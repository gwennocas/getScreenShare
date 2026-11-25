# getScreenShare

App de desktop para compartilhar a tela em tempo real, com Tauri no Windows, Electron no Linux e WebRTC (LiveKit).
Cada pessoa entra numa sala pelo nome, escolhe uma tela ou janela e os participantes assistem em tempo real. A latência depende da rede, do codec e do dispositivo.

## Cliente desktop

O cliente Windows usa Tauri. No Linux, o app usa Electron e Chromium para manter o suporte a WebRTC do LiveKit. O WebKitGTK usado pelo Tauri não fornece `RTCPeerConnection` neste ambiente, então o LiveKit recusa a conexão antes de entrar na sala. O cliente Electron já trata captura pelo portal PipeWire no Wayland e pelo seletor de tela no X11. A shell Tauri e a migração do Windows estão em [`experiments/tauri-capture`](experiments/tauri-capture/README.md).

Durante o compartilhamento, use **Trocar tela** para escolher outra tela ou janela sem parar a transmissão. Confirme a nova fonte no seletor; o áudio, a câmera e a qualidade atual são mantidos. No Wayland, a escolha acontece no portal do sistema. Se cancelar a escolha ou a nova captura falhar, a fonte atual continua sendo transmitida.

O botão **X** esconde a janela na bandeja do sistema e mantém a sala, a tela, a câmera e o áudio funcionando. Clique no ícone da bandeja ou em **Abrir getScreenShare** no menu dele para voltar. Para encerrar o aplicativo e a transmissão, escolha **Sair** nesse menu. Se o ícone da bandeja não puder ser criado, o **X** fecha normalmente.

O app mantém uma única instância por perfil. Abrir o executável ou o atalho novamente restaura a janela existente, mesmo se ela estiver na bandeja, sem criar outra sala ou outro ícone.

Na sala, **Ligar câmera** permite escolher uma câmera disponível e publicá-la ao lado das telas. Se uma câmera não aparecer na lista, use **Detectar outras câmeras** para autorizar o acesso e atualizar os dispositivos. A câmera pode continuar ligada enquanto você compartilha a tela. **Desligar câmera** interrompe a captura; o microfone não é ativado.

## Instalar

Baixe a versão desejada em [Releases](https://github.com/gwennocas/getScreenShare/releases).

- **Windows x64:** execute `getScreenShare-Tauri-Setup-vVERSÃO.exe`.
- **Linux x64:** baixe `getScreenShare-vVERSÃO-linux-x86_64.AppImage`, permita a execução nas propriedades do arquivo e abra-o. Pelo terminal, use `chmod +x getScreenShare-*.AppImage` e execute o arquivo baixado. Algumas distribuições precisam do pacote de compatibilidade do FUSE 2 para abrir AppImages.

Na primeira execução, configure a URL do servidor de tokens e o código de acesso em **Configurações → Geral → Conexão**.

| Recurso | Windows x64 | Linux x64 |
| --- | --- | --- |
| Tela e janela | Seletor do sistema | Seletor do sistema e portal no Wayland |
| Câmera | Disponível | Disponível |
| Codificação por GPU | Conforme WebView2, driver e codec | VA-API, conforme Chromium, driver e codec |
| Áudio do computador com exclusão do Discord | WASAPI, Windows compatível | Indisponível |
| Atualização pelo aplicativo | Disponível | Instalação manual da nova versão |

O seletor de captura Wayland depende do backend de portal correspondente. A captura e a transmissão no Electron ainda devem ser validadas em cada ambiente Wayland e com codificação por GPU.

## Rodar durante o desenvolvimento

1. Instale Node.js (versão LTS) e rode `npm ci` na raiz do repositório.
2. No Windows, instale Rust estável, o Tauri CLI e os pré-requisitos de compilação; rode `npm run tauri:dev`.
3. No Linux, rode `npm run electron:dev`.
4. Configure um servidor de tokens conforme as instruções abaixo, ou obtenha a URL e o código de acesso de quem já hospeda um. Informe ambos em **Configurações → Geral → Conexão**.

Para testar a transmissão, abra o app em dois computadores, entre na mesma sala nos dois e compartilhe a tela em um deles. Deixe "Compartilhar o áudio do computador" desmarcado se os computadores reproduzirem o mesmo áudio no ambiente, para evitar retorno.

## Usar com amigos: servidor de tokens

O aplicativo usa exclusivamente um servidor de tokens. As credenciais do LiveKit ficam no servidor; o app não lê arquivos `.env` nem gera tokens localmente.

A pasta `server/` tem um servidor pequeno que guarda a chave e entrega os tokens. Quem usa o app só precisa do endereço do servidor e de um código de acesso. Para hospedar outra instância no Render, clone ou faça um fork deste repositório, publique com suas próprias credenciais e configure a nova URL no app. Todos que usam essa instância compartilham o mesmo endereço de servidor.

1. Entre em `server/`, copie `.env.example` para `.env` e preencha os dados do LiveKit e o `ACCESS_CODE`.
2. Rode `npm install` e `npm start` (porta 8787 por padrão). Teste abrindo `http://localhost:8787/health`.
3. Para publicar no Render, este projeto inclui `render.yaml`. Envie o conteúdo desta pasta para um repositório GitHub; no Render, escolha **New + → Blueprint**, conecte o repositório e informe as variáveis secretas solicitadas. A configuração aponta para `server/`, usa `npm ci` e `npm start`, configura `/health` como health check e ativa `TRUST_PROXY=1`. Alternativamente, crie um Web Service manualmente com Root Directory `server`, Build Command `npm ci`, Start Command `npm start` e Health Check Path `/health`.
4. Depois do deploy, teste `https://SEU-SERVICO.onrender.com/health`. A resposta deve ser `{"ok":true}`.
5. No app, abra **Configurações → Geral → Conexão**, cole somente a URL base do serviço (por exemplo, `https://SEU-SERVICO.onrender.com`, sem `/token`) e o mesmo código definido em `ACCESS_CODE`, e clique em "Testar servidor".

Defina `LIVEKIT_URL`, `LIVEKIT_API_KEY` e `LIVEKIT_API_SECRET` com as credenciais do seu projeto no LiveKit Cloud, além de um `ACCESS_CODE` longo e difícil de adivinhar. O servidor se recusa a iniciar se `ACCESS_CODE` estiver ausente. Nunca coloque o API Secret no repositório GitHub nem no app dos seus amigos. O servidor limita 30 pedidos por minuto por endereço e só aceita nomes de sala simples.

**Nota do plano gratuito:** o Render suspende serviços web gratuitos após 15 minutos sem tráfego e pode levar cerca de um minuto para acordar no próximo pedido. A primeira tentativa de conectar ao app depois desse período pode demorar.

## Gerar o instalador

Rode `npm run tauri:build` no Windows para gerar o instalador NSIS. No Linux, rode `npm run dist:linux` para gerar o AppImage Electron em `dist/`. Depois de instalar, configure a URL do servidor de tokens e o código de acesso. Nenhum arquivo `.env` é necessário no computador de quem usa o app.

### Atualizar o app

**No Windows instalado**, abra **Configurações → Atualizações** e toque em **Verificar atualizações**. Se houver uma versão nova, toque em **Baixar atualização**. Quando o download terminar, toque em **Reiniciar e instalar**. O app também verifica atualizações ao abrir, mas só baixa e instala depois desses cliques.

**No Linux**, baixe o AppImage da nova versão em Releases e substitua o arquivo anterior. As configurações ficam no perfil do usuário.

### Publicar uma versão

1. Atualize a versão em `package.json`, `package-lock.json`, `experiments/tauri-capture/Cargo.toml` e `experiments/tauri-capture/tauri.conf.json`; execute `npm test`, compile o Tauri no Windows e gere o AppImage Electron no Linux.
2. No GitHub, crie uma release com a tag `vVERSÃO`, apontando para o commit da versão.
3. Ao publicar a release, o workflow `.github/workflows/release.yml` gera o instalador Tauri do Windows e o AppImage Electron do Linux. Acompanhe a execução na aba **Actions**; os downloads ficam prontos quando o workflow terminar.

O workflow usa o token automático do GitHub Actions para anexar os pacotes à release publicada.

### Compartilhamento no Wayland

Em uma sessão Wayland, clique em **Compartilhar tela** e depois em **Começar a compartilhar**. Escolha a tela ou janela no seletor do sistema (PipeWire/xdg-desktop-portal). É necessário ter um backend do portal instalado para seu ambiente de desktop. O áudio do computador é desativado no Linux; a captura WASAPI com exclusão do Discord continua disponível no Windows.

#### Preparar o portal

Instale PipeWire, WirePlumber, `xdg-desktop-portal` e o backend do seu desktop: `xdg-desktop-portal-hyprland` no Hyprland, `xdg-desktop-portal-gnome` no GNOME ou `xdg-desktop-portal-kde` no KDE Plasma. Mantenha também o backend de seleção de arquivos usado pelo seu desktop.

Se o backend foi instalado com a sessão já aberta, reinicie a sessão ou reinicie os serviços de portal. No Hyprland:

```sh
systemctl --user restart xdg-desktop-portal-hyprland.service
systemctl --user restart xdg-desktop-portal.service
```

Se nem o navegador abrir o seletor de compartilhamento, consulte os logs:

```sh
journalctl --user -b -u xdg-desktop-portal -u xdg-desktop-portal-hyprland
```

A mensagem `No such interface org.freedesktop.portal.ScreenCast` indica que o portal não carregou o suporte à captura. Confira o backend instalado e se o ambiente da sessão contém `XDG_CURRENT_DESKTOP` e `WAYLAND_DISPLAY`. Veja também a [documentação de compartilhamento do Hyprland](https://wiki.hypr.land/useful-utilities/screen-sharing/).

### Áudio sem o Discord

Ao marcar **Compartilhar o áudio do computador**, o app captura os sons do sistema pelo WASAPI, excluindo o Discord e seus processos filhos. O recurso exige Windows x64, Discord aberto e Windows 10 versão 2004 ou mais recente. Se a captura filtrada não iniciar, a tela continua sendo compartilhada sem áudio.

## Onde mexer na responsividade

- Configurações do app: codec (H.264, VP8, VP9), simulcast, reduzir atraso de reprodução.
- `renderer/js/settings.js`: resolução, fps e bitrate de cada qualidade.
- `renderer/js/app.js`, em `startShare()`: `contentHint` e `degradationPreference`. Em `addTile()`: `jitterBufferTarget`.

## Estatísticas e codificação por placa de vídeo

Em cada tela recebida, o botão de informação alterna entre resumo, detalhes completos e escondido. Enquanto você compartilha, o painel "Seu envio" mostra, entre outras coisas:

- Codificação: "placa de vídeo" ou "processador". Vem do sinal `powerEfficientEncoder` do navegador.
- Encoder: o nome da implementação em uso (por exemplo, `OpenH264` é por software).
- Limite: o que está segurando a qualidade (processador ou internet).

O app prioriza H.264 High quando o WebView anuncia suporte a esse perfil, preservando os demais para compatibilidade com o servidor. O painel "Seu envio" mostra o perfil negociado e o encoder informado pelo navegador. O LiveKit está fixado na versão 2.23.0 porque a configuração de preferência usa seu evento de criação do sender e o acesso ao transceiver do transporte.

No Linux, o driver e o WebViewGTK precisam oferecer codificação para o codec escolhido; confira o encoder mostrado no painel "Seu envio" durante a transmissão.

Se aparecer "processador", teste desligar o simulcast nas configurações e compartilhar de novo. A disponibilidade geral da GPU não garante que o perfil negociado ou a configuração de transmissão possam usar hardware.

## Estrutura

- `experiments/tauri-capture/src/`: shell Tauri, bandeja, áudio WASAPI e comandos nativos
- `renderer/`: interface (`index.html`, `style.css`, `js/`, `fonts/`)
- `scripts/prepare-tauri-frontend.js`: prepara o renderer e copia o bundle LiveKit
- `test/`: testes do renderer e da lógica de captura
- `.github/workflows/release.yml`: builds Tauri para Windows e Electron para Linux
- `server/`: servidor de tokens
- `build/`: ícone do app
