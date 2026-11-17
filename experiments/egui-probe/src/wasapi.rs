// Adaptado de wasapi-loopback; licenças em third_party/wasapi-loopback.

#![cfg(windows)]

use std::mem::{size_of, ManuallyDrop};
use std::sync::mpsc::{channel, Sender};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;

use windows::core::{implement, w, IUnknown, Interface, GUID, HRESULT, PCWSTR};
use windows::Win32::Foundation::{
    CloseHandle, GetLastError, ERROR_PROC_NOT_FOUND, E_FAIL, HANDLE, S_OK, WAIT_OBJECT_0,
    WAIT_TIMEOUT,
};
use windows::Win32::Media::Audio::{
    ActivateAudioInterfaceAsync, IActivateAudioInterfaceAsyncOperation,
    IActivateAudioInterfaceCompletionHandler, IActivateAudioInterfaceCompletionHandler_Impl,
    IAudioCaptureClient, IAudioClient, AUDCLNT_BUFFERFLAGS_SILENT, AUDCLNT_SHAREMODE_SHARED,
    AUDCLNT_STREAMFLAGS_AUTOCONVERTPCM, AUDCLNT_STREAMFLAGS_EVENTCALLBACK,
    AUDCLNT_STREAMFLAGS_LOOPBACK, AUDCLNT_STREAMFLAGS_SRC_DEFAULT_QUALITY,
    AUDIOCLIENT_ACTIVATION_PARAMS, AUDIOCLIENT_ACTIVATION_PARAMS_0,
    AUDIOCLIENT_ACTIVATION_TYPE_PROCESS_LOOPBACK, AUDIOCLIENT_PROCESS_LOOPBACK_PARAMS,
    PROCESS_LOOPBACK_MODE_EXCLUDE_TARGET_PROCESS_TREE, VIRTUAL_AUDIO_DEVICE_PROCESS_LOOPBACK,
    WAVEFORMATEX, WAVEFORMATEXTENSIBLE, WAVEFORMATEXTENSIBLE_0,
};
use windows::Win32::Media::KernelStreaming::WAVE_FORMAT_EXTENSIBLE;
use windows::Win32::Media::Multimedia::KSDATAFORMAT_SUBTYPE_IEEE_FLOAT;
use windows::Win32::System::Com::StructuredStorage::PROPVARIANT;
use windows::Win32::System::Com::{CoInitializeEx, CoUninitialize, BLOB, COINIT_MULTITHREADED};
use windows::Win32::System::LibraryLoader::{GetProcAddress, LoadLibraryW};
use windows::Win32::System::Threading::{
    CreateEventW, OpenProcess, SetEvent, WaitForSingleObject, INFINITE, PROCESS_SYNCHRONIZE,
};
use windows::Win32::System::Variant::VT_BLOB;

const SAMPLE_RATE: u32 = 48_000;
const CHANNELS: u16 = 2;
const BITS_PER_SAMPLE: u16 = 32;
const BLOCK_ALIGN: u16 = CHANNELS * BITS_PER_SAMPLE / 8; // 8 bytes/frame
const AVG_BYTES_PER_SEC: u32 = SAMPLE_RATE * BLOCK_ALIGN as u32; // 384000

const SPEAKER_STEREO_MASK: u32 = 0x3;

const FRAMES_PER_CHUNK: usize = 480;
const BYTES_PER_FRAME: usize = (CHANNELS as usize) * (BITS_PER_SAMPLE as usize / 8); // 8
const CHUNK_BYTES: usize = FRAMES_PER_CHUNK * BYTES_PER_FRAME; // 3840

type ChunkTsfn = Arc<dyn Fn(Vec<u8>) + Send + Sync + 'static>;

const STOP_JOIN_TIMEOUT_MS: u64 = 1500;

fn build_wave_format() -> WAVEFORMATEXTENSIBLE {
    WAVEFORMATEXTENSIBLE {
        Format: WAVEFORMATEX {
            wFormatTag: WAVE_FORMAT_EXTENSIBLE as u16,
            nChannels: CHANNELS,
            nSamplesPerSec: SAMPLE_RATE,
            wBitsPerSample: BITS_PER_SAMPLE,
            nBlockAlign: BLOCK_ALIGN,
            nAvgBytesPerSec: AVG_BYTES_PER_SEC,
            cbSize: (size_of::<WAVEFORMATEXTENSIBLE>() - size_of::<WAVEFORMATEX>()) as u16,
        },
        Samples: WAVEFORMATEXTENSIBLE_0 {
            wValidBitsPerSample: BITS_PER_SAMPLE,
        },
        dwChannelMask: SPEAKER_STEREO_MASK,
        SubFormat: KSDATAFORMAT_SUBTYPE_IEEE_FLOAT,
    }
}

enum ActivationResult {
    Activated(IAudioClient),
    Unsupported,
}

#[implement(IActivateAudioInterfaceCompletionHandler)]
struct CompletionHandler {
    done: HANDLE,
}

impl IActivateAudioInterfaceCompletionHandler_Impl for CompletionHandler_Impl {
    fn ActivateCompleted(
        &self,
        _operation: windows::core::Ref<'_, IActivateAudioInterfaceAsyncOperation>,
    ) -> windows::core::Result<()> {
        unsafe {
            let _ = SetEvent(self.done);
        }
        Ok(())
    }
}

type ActivateAudioInterfaceAsyncFn = unsafe extern "system" fn(
    deviceinterfacepath: PCWSTR,
    riid: *const GUID,
    activationparams: *const PROPVARIANT,
    completionhandler: *mut core::ffi::c_void,
    activationoperation: *mut *mut core::ffi::c_void,
) -> HRESULT;

fn process_loopback_entrypoint_present() -> bool {
    unsafe {
        let module = match LoadLibraryW(w!("mmdevapi.dll")) {
            Ok(h) if !h.is_invalid() => h,
            _ => return false,
        };
        let proc = GetProcAddress(module, windows::core::s!("ActivateAudioInterfaceAsync"));
        if proc.is_none() {
            let _ = GetLastError() == ERROR_PROC_NOT_FOUND;
            return false;
        }
        let _resolved: ActivateAudioInterfaceAsyncFn =
            std::mem::transmute::<_, ActivateAudioInterfaceAsyncFn>(proc.unwrap());
        true
    }
}

unsafe fn activate_exclude_tree(exclude_root_pid: u32) -> ActivationResult {
    if !process_loopback_entrypoint_present() {
        return ActivationResult::Unsupported;
    }

    let mut activation_params = AUDIOCLIENT_ACTIVATION_PARAMS {
        ActivationType: AUDIOCLIENT_ACTIVATION_TYPE_PROCESS_LOOPBACK,
        Anonymous: AUDIOCLIENT_ACTIVATION_PARAMS_0 {
            ProcessLoopbackParams: AUDIOCLIENT_PROCESS_LOOPBACK_PARAMS {
                TargetProcessId: exclude_root_pid,
                ProcessLoopbackMode: PROCESS_LOOPBACK_MODE_EXCLUDE_TARGET_PROCESS_TREE,
            },
        },
    };

    // O PROPVARIANT aponta para a pilha e não deve liberar essa memória.
    let mut prop = ManuallyDrop::new(PROPVARIANT::default());
    {
        let pv = &mut prop.Anonymous.Anonymous;
        pv.vt = VT_BLOB;
        pv.Anonymous.blob = BLOB {
            cbSize: size_of::<AUDIOCLIENT_ACTIVATION_PARAMS>() as u32,
            pBlobData: &mut activation_params as *mut _ as *mut u8,
        };
    }

    let done = match CreateEventW(None, true, false, PCWSTR::null()) {
        Ok(h) => h,
        Err(_) => return ActivationResult::Unsupported,
    };
    let handler: IActivateAudioInterfaceCompletionHandler = CompletionHandler { done }.into();

    let operation: IActivateAudioInterfaceAsyncOperation = match ActivateAudioInterfaceAsync(
        VIRTUAL_AUDIO_DEVICE_PROCESS_LOOPBACK,
        &IAudioClient::IID,
        Some(&*prop),
        &handler,
    ) {
        Ok(op) => op,
        Err(_) => {
            let _ = CloseHandle(done);
            return ActivationResult::Unsupported;
        }
    };

    let wait = WaitForSingleObject(done, INFINITE);
    let _ = CloseHandle(done);
    if wait != WAIT_OBJECT_0 {
        return ActivationResult::Unsupported;
    }

    let mut activate_hr: HRESULT = E_FAIL;
    let mut activated_iface: Option<IUnknown> = None;
    if operation
        .GetActivateResult(&mut activate_hr, &mut activated_iface)
        .is_err()
    {
        return ActivationResult::Unsupported;
    }
    if activate_hr != S_OK {
        return ActivationResult::Unsupported;
    }
    let audio_client: IAudioClient = match activated_iface.and_then(|u| u.cast().ok()) {
        Some(c) => c,
        None => return ActivationResult::Unsupported,
    };

    let wfx = build_wave_format();
    let stream_flags = AUDCLNT_STREAMFLAGS_LOOPBACK
        | AUDCLNT_STREAMFLAGS_EVENTCALLBACK
        | AUDCLNT_STREAMFLAGS_AUTOCONVERTPCM
        | AUDCLNT_STREAMFLAGS_SRC_DEFAULT_QUALITY;
    if audio_client
        .Initialize(
            AUDCLNT_SHAREMODE_SHARED,
            stream_flags, // <-- StreamFlags (2nd param): AUTOCONVERTPCM lives here.
            0,            // hnsBufferDuration (engine default in event-driven shared mode)
            0,            // hnsPeriodicity (0 for event-driven shared mode)
            &wfx as *const _ as *const WAVEFORMATEX,
            None,
        )
        .is_err()
    {
        return ActivationResult::Unsupported;
    }

    ActivationResult::Activated(audio_client)
}

struct CaptureSession {
    stop_event: Arc<SendHandle>,
    join: Option<JoinHandle<()>>,
}

unsafe impl Send for CaptureSession {}

struct SendHandle(HANDLE);
// O evento é um objeto do Windows seguro para sinalização entre threads.
unsafe impl Send for SendHandle {}
unsafe impl Sync for SendHandle {}
impl Drop for SendHandle {
    fn drop(&mut self) {
        unsafe {
            let _ = CloseHandle(self.0);
        }
    }
}

static SESSION: Mutex<Option<CaptureSession>> = Mutex::new(None);

unsafe fn run_capture_loop(
    audio_client: IAudioClient,
    on_chunk: ChunkTsfn,
    stop_event: HANDLE,
    excluded_process: HANDLE,
    ready: Sender<bool>,
) {
    let audio_event = match CreateEventW(None, false, false, PCWSTR::null()) {
        Ok(h) => h,
        Err(_) => return,
    };
    if audio_client.SetEventHandle(audio_event).is_err() {
        let _ = CloseHandle(audio_event);
        return;
    }

    let capture: IAudioCaptureClient = match audio_client.GetService() {
        Ok(c) => c,
        Err(_) => {
            let _ = CloseHandle(audio_event);
            return;
        }
    };

    if audio_client.Start().is_err() {
        let _ = CloseHandle(audio_event);
        return;
    }

    if WaitForSingleObject(excluded_process, 0) != WAIT_TIMEOUT {
        let _ = audio_client.Stop();
        let _ = CloseHandle(audio_event);
        return;
    }
    let _ = ready.send(true);
    let mut acc: Vec<u8> = Vec::with_capacity(CHUNK_BYTES * 2);

    'capture: loop {
        if WaitForSingleObject(stop_event, 0) == WAIT_OBJECT_0
            || WaitForSingleObject(excluded_process, 0) != WAIT_TIMEOUT
        {
            break;
        }

        let _ = WaitForSingleObject(audio_event, 100);

        loop {
            let packet_frames = match capture.GetNextPacketSize() {
                Ok(n) => n,
                Err(_) => break 'capture,
            };
            if packet_frames == 0 {
                break;
            }

            let mut data_ptr: *mut u8 = std::ptr::null_mut();
            let mut num_frames: u32 = 0;
            let mut flags: u32 = 0;
            if capture
                .GetBuffer(&mut data_ptr, &mut num_frames, &mut flags, None, None)
                .is_err()
            {
                break 'capture;
            }

            let frame_count = num_frames as usize;
            let byte_count = frame_count * BYTES_PER_FRAME;
            if (flags & AUDCLNT_BUFFERFLAGS_SILENT.0 as u32) != 0 || data_ptr.is_null() {
                acc.resize(acc.len() + byte_count, 0u8);
            } else {
                let slice = std::slice::from_raw_parts(data_ptr, byte_count);
                acc.extend_from_slice(slice);
            }

            if capture.ReleaseBuffer(num_frames).is_err() {
                break 'capture;
            }

            while acc.len() >= CHUNK_BYTES {
                let chunk: Vec<u8> = acc.drain(..CHUNK_BYTES).collect();
                on_chunk(chunk);
            }
        }
    }

    let _ = audio_client.Stop();
    let _ = CloseHandle(audio_event);
}

pub fn start(exclude_root_pid: u32, on_chunk: ChunkTsfn) -> bool {
    let mut guard = SESSION.lock().unwrap_or_else(|p| p.into_inner());
    if guard.is_some() {
        return true;
    }

    let stop_event = match unsafe { CreateEventW(None, true, false, PCWSTR::null()) } {
        Ok(h) => h,
        Err(_) => return false,
    };

    let excluded_process =
        match unsafe { OpenProcess(PROCESS_SYNCHRONIZE, false, exclude_root_pid) } {
            Ok(handle) => Arc::new(SendHandle(handle)),
            Err(_) => {
                unsafe {
                    let _ = CloseHandle(stop_event);
                }
                return false;
            }
        };
    let (tx, rx): (Sender<bool>, _) = channel();
    let stop_event = Arc::new(SendHandle(stop_event));
    let stop_event_for_thread = stop_event.clone();

    let join = std::thread::spawn(move || {
        let stop_handle = stop_event_for_thread;

        let com = unsafe { CoInitializeEx(None, COINIT_MULTITHREADED) };
        let com_ok = com.is_ok();

        let activation = unsafe { activate_exclude_tree(exclude_root_pid) };
        match activation {
            ActivationResult::Activated(client) => {
                unsafe {
                    run_capture_loop(client, on_chunk, stop_handle.0, excluded_process.0, tx)
                };
            }
            ActivationResult::Unsupported => {
                let _ = tx.send(false);
            }
        }

        if com_ok {
            unsafe { CoUninitialize() };
        }
    });

    let supported = rx.recv().unwrap_or(false);
    if !supported {
        unsafe {
            let _ = SetEvent(stop_event.0);
        }
        let _ = join.join();
        return false;
    }

    *guard = Some(CaptureSession {
        stop_event,
        join: Some(join),
    });
    true
}

pub fn stop() {
    let session = {
        let mut guard = SESSION.lock().unwrap_or_else(|p| p.into_inner());
        guard.take()
    };

    let Some(mut session) = session else {
        return; // idempotent: nothing running.
    };

    unsafe {
        let _ = SetEvent(session.stop_event.0);
    }

    if let Some(join) = session.join.take() {
        let deadline =
            std::time::Instant::now() + std::time::Duration::from_millis(STOP_JOIN_TIMEOUT_MS);
        loop {
            if join.is_finished() {
                let _ = join.join();
                break;
            }
            if std::time::Instant::now() >= deadline {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{
        os::windows::process::CommandExt,
        process::{Command, Stdio},
        sync::mpsc::RecvTimeoutError,
        time::{Duration, Instant},
    };

    #[test]
    fn refuses_nonexistent_excluded_process() {
        assert!(!start(u32::MAX, Arc::new(|_| {})));
    }

    #[test]
    #[ignore = "requer dispositivo de áudio no Windows"]
    fn stops_when_excluded_process_exits() {
        let mut child = Command::new(std::env::var_os("ComSpec").unwrap())
            .args(["/c", "ping", "127.0.0.1", "-n", "30"])
            .creation_flags(0x08000000)
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .unwrap();
        let (tx, rx) = channel();
        let started = start(
            child.id(),
            Arc::new(move |_: Vec<u8>| {
                let _ = tx.send(());
            }),
        );
        child.kill().unwrap();
        child.wait().unwrap();
        assert!(started, "a captura de teste não iniciou");
        let deadline = Instant::now() + Duration::from_secs(5);
        loop {
            match rx.recv_timeout(Duration::from_millis(100)) {
                Err(RecvTimeoutError::Disconnected) => break,
                Err(RecvTimeoutError::Timeout) if Instant::now() >= deadline => {
                    panic!("captura continuou após o processo excluído terminar")
                }
                _ => {}
            }
        }
        stop();
    }
}
