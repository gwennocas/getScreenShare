# Cliente Rust para Windows

Cliente nativo experimental em egui para Windows x64. Usa o mesmo servidor de tokens e o LiveKit já usado pelo aplicativo Electron. Conecta a uma sala, captura tela ou janela na resolução original, permite selecionar 30, 60 ou a taxa do monitor no início, recebe vídeo e áudio, permite ajustar o zoom e pode enviar áudio do computador excluindo a árvore atual do Discord. As credenciais ficam nas configurações do Windows; o código de acesso é protegido com DPAPI.

Este é o cliente nativo para Windows; Electron continua sendo a versão para Linux. O cliente inclui bandeja, estatísticas do LiveKit, atualização pelo app e migração de configurações não secretas do Electron. O código de acesso precisa ser digitado novamente e depois é protegido com DPAPI. A codificação atual usa OpenH264 por software nesta máquina.

## Compilar

No Windows x64, instale Rust estável e as ferramentas C++ do Visual Studio. Execute a partir desta pasta para carregar a configuração do runtime C++:

```powershell
$env:CARGO_TARGET_DIR = Join-Path $env:TEMP "gss-cargo"
cargo run --release
```

O primeiro build baixa os componentes nativos WebRTC. O diretório de compilação curto evita caminhos longos no compilador C++. Com esse comando, o executável sai em `%TEMP%\gss-cargo\release\getscreenshare-egui-probe.exe`.

As configurações ficam em `%APPDATA%\getScreenShare-rust\settings.json`. Ao salvar no cliente Electron do Windows, nome, sala, servidor, qualidade e preferência de áudio são exportados para importação automática no primeiro uso do cliente Rust. O código de acesso não é copiado por segurança; informe-o novamente. O cliente reutiliza o servidor de tokens e o LiveKit.

## Verificações

`cargo test` executa os testes unitários. Testes de integração adicionais estão marcados como ignorados porque precisam de LiveKit local, monitor interativo e, para áudio, dispositivo de saída e Discord. A captura convertida em quadro, o envio/recepção de vídeo, áudio recebido, loopback WASAPI e parada ao fechar o processo excluído foram exercitados localmente.

Em uma comparação ociosa neste computador, o grupo Electron usou 276,8 MiB em quatro processos, enquanto o Rust usou 91,8 MiB em um processo, cerca de 67% menos RAM. É uma amostra após cinco segundos sem compartilhar; ainda falta comparar durante uma transmissão. O teste de loopback valida o modo de exclusão WASAPI, mas ainda precisa de uma prova de áudio com sinais conhecidos para confirmar a exclusão audível do Discord.

Enumeração de câmera precisa de validação com uma webcam real; o fluxo de atualização precisa de uma release publicada. O loopback usa código adaptado de `wasapi-loopback`; licenças e avisos estão em `third_party/wasapi-loopback`.
