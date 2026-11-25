use std::{
    os::windows::process::CommandExt,
    process::{Command, Stdio},
    thread,
    time::{Duration, Instant},
};

pub fn discord_pid() -> Result<u32, String> {
    let root = std::env::var_os("SystemRoot").ok_or("Diretório do Windows indisponível.")?;
    let powershell =
        std::path::PathBuf::from(root).join("System32/WindowsPowerShell/v1.0/powershell.exe");
    let script = "$names=@('Discord.exe','DiscordPTB.exe','DiscordCanary.exe'); $p=@(Get-CimInstance Win32_Process -ErrorAction Stop | Where-Object { $names -contains $_.Name }); $ids=@($p | ForEach-Object { [int]$_.ProcessId }); $roots=@($p | Where-Object { $ids -notcontains [int]$_.ParentProcessId }); if ($roots.Count -ne 1) { exit 1 }; [int]$roots[0].ProcessId";
    let mut process = Command::new(powershell)
        .args([
            "-NoLogo",
            "-NoProfile",
            "-NonInteractive",
            "-Command",
            script,
        ])
        .creation_flags(0x08000000)
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|_| "Não foi possível identificar o Discord; áudio desativado.")?;
    let deadline = Instant::now() + Duration::from_secs(8);
    loop {
        match process.try_wait() {
            Ok(Some(_)) => break,
            Ok(None) if Instant::now() < deadline => thread::sleep(Duration::from_millis(30)),
            _ => {
                let _ = process.kill();
                let _ = process.wait();
                return Err("Não foi possível identificar o Discord; áudio desativado.".into());
            }
        }
    }
    let output = process
        .wait_with_output()
        .map_err(|_| "Não foi possível identificar o Discord; áudio desativado.")?;
    if !output.status.success() {
        return Err(
            "Abra somente uma instalação do Discord para compartilhar áudio com segurança.".into(),
        );
    }
    let pid = String::from_utf8_lossy(&output.stdout)
        .trim()
        .parse::<u32>()
        .map_err(|_| "Não foi possível identificar o Discord; áudio desativado.")?;
    if pid == 0 {
        return Err("Não foi possível identificar o Discord; áudio desativado.".into());
    }
    Ok(pid)
}

#[cfg(test)]
mod tests {
    #[test]
    #[ignore = "requer Discord aberto no Windows"]
    fn finds_a_single_discord_root() {
        assert!(super::discord_pid().unwrap() > 0);
    }
}
