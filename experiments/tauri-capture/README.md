# Cliente Tauri

Esta pasta contém o novo shell desktop de getScreenShare. Ele reutiliza o renderer LiveKit em `renderer/`, apresenta a interface Material 3 e usa o seletor de captura do WebView. No Windows, o loopback WASAPI com exclusão da árvore do Discord reutiliza o módulo do cliente Rust anterior. As configurações locais são protegidas com DPAPI e importadas do cliente egui ou do Electron.

## Desenvolvimento

Instale Node.js, Rust estável, as ferramentas C++ do Visual Studio no Windows e os pré-requisitos Tauri da sua distribuição Linux. Instale o Tauri CLI com `cargo install tauri-cli --version "^2" --locked`. Na raiz do repositório, rode `npm ci`. Depois use `npm run tauri:dev`.

`npm run tauri:prepare` copia o renderer para esta pasta e inclui o bundle UMD do LiveKit. O comando de desenvolvimento e a compilação de release executam essa preparação automaticamente.

## Recursos integrados

- Conexão ao servidor de tokens e ao LiveKit
- Compartilhamento e troca de tela ou janela pelo seletor do sistema
- Câmera, áudio recebido, estatísticas e codificação por GPU do WebView
- Áudio de computador com exclusão do Discord no Windows x64
- Bandeja, instância única e atualização do instalador no Windows
- Importação de preferências do cliente egui no Windows

## Antes de publicar

Compile e execute em Windows e Linux. Valide captura e troca de fonte, câmera, transmissão LiveKit, áudio recebido, áudio WASAPI excluindo o Discord, bandeja, segunda instância, persistência das configurações, atualização e instalação limpa. O cliente Linux depende do portal de captura de tela da sessão Wayland.
