# getScreenShare

App de desktop para compartilhar a tela em tempo real, feito com Electron e WebRTC (LiveKit).
Cada pessoa entra numa sala pelo nome, escolhe uma tela ou janela e os participantes assistem em tempo real. A latência depende da rede, do codec e do dispositivo.

Na sala, **Ligar câmera** permite escolher uma câmera disponível e publicá-la ao lado das telas. Se uma câmera não aparecer na lista, use **Detectar outras câmeras** para autorizar o acesso e atualizar os dispositivos. A câmera pode continuar ligada enquanto você compartilha a tela. **Desligar câmera** interrompe a captura; o microfone não é ativado.

## Instalar

Baixe a versão desejada em [Releases](https://github.com/gwennocas/getScreenShare/releases).

- **Windows x64:** execute `getScreenShare-Setup-VERSÃO.exe`.
- **Linux x64:** baixe `getScreenShare-VERSÃO-linux-x86_64.AppImage`, permita a execução nas propriedades do arquivo e abra-o. Pelo terminal, use `chmod +x getScreenShare-*.AppImage` e execute o arquivo baixado. Algumas distribuições precisam do pacote de compatibilidade do FUSE 2 para abrir AppImages.

Na primeira execução, configure a URL do servidor de tokens e o código de acesso em **Configurações → Geral → Conexão**.

| Recurso | Windows x64 | Linux x64 |
| --- | --- | --- |
| Tela e janela | Seletor do aplicativo | Portal do sistema no Wayland; seletor do aplicativo no X11 |
| Câmera | Disponível | Disponível |
| Codificação por GPU | Conforme driver e codec | VA-API, conforme driver e codec |
| Áudio do computador com exclusão do Discord | WASAPI, Windows compatível | Indisponível |
| Atualização pelo aplicativo | Disponível | Instalação manual da nova versão |

A captura Wayland e a codificação por GPU foram confirmadas em Arch Linux com Hyprland e Radeon RX 6600. Outros desktops precisam do backend de portal correspondente.

## Rodar durante o desenvolvimento (Windows e Linux)

1. Instale o Node.js (versão LTS) em https://nodejs.org
2. Para hospedar seu próprio servidor, crie um projeto no LiveKit Cloud (https://cloud.livekit.io) e obtenha a URL (`wss://...`), a API Key e o API Secret. Quem usa um servidor existente não precisa dessas credenciais.
3. Abra um terminal nesta pasta e rode `npm install`.
4. Configure um servidor de tokens conforme as instruções abaixo, ou obtenha a URL e o código de acesso de quem já hospeda um.
5. Rode `npm start` e informe a URL e o código em **Configurações → Geral → Conexão**.

Para testar sozinha, abra duas janelas do app (dois terminais com `npm start`), entre na mesma sala nas duas e compartilhe numa delas. Deixe "Compartilhar o áudio do computador" desmarcado nesse teste, senão o som volta em loop.

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

No Linux, rode `npm run dist` para gerar o AppImage ou `npm run dist:deb` para gerar um pacote `.deb` (requer `libcrypt.so.1` no sistema de compilação). No Windows, rode `npm run dist:win` para gerar o instalador NSIS. Os arquivos aparecem na pasta `dist/`. Depois de instalar, configure a URL do servidor de tokens e o código de acesso. Nenhum arquivo `.env` é necessário no computador de quem usa o app.

### Atualizar o app

**No Windows instalado**, abra **Configurações → Atualizações** e toque em **Verificar atualizações**. Se houver uma versão nova, toque em **Baixar atualização**. Quando o download terminar, toque em **Reiniciar e instalar**. O app também verifica atualizações ao abrir, mas só baixa e instala depois desses cliques.

**No Linux**, baixe o AppImage da nova versão em Releases e substitua o arquivo anterior. As configurações ficam no perfil do usuário.

### Publicar uma versão

1. Atualize `version` em `package.json` e `package-lock.json`, execute `npm test` e envie as alterações ao repositório.
2. No GitHub, crie uma release com a tag `vVERSÃO`, apontando para o commit da versão.
3. Ao publicar a release, o workflow `.github/workflows/release.yml` gera o instalador Windows e o AppImage Linux e anexa os pacotes e metadados de atualização. Acompanhe a execução na aba **Actions**; os downloads ficam prontos quando o workflow terminar.

O workflow usa o token automático do GitHub Actions. Para publicar localmente, configure `GH_TOKEN` com permissão de escrita no repositório e rode `npm run release` no sistema para o qual deseja gerar o pacote.

### Compartilhamento no Wayland

Em uma sessão Wayland, clique em **Compartilhar tela** e depois em **Começar a compartilhar**. Escolha a tela ou janela no portal do sistema (PipeWire/xdg-desktop-portal). É necessário ter um backend do portal instalado para seu ambiente de desktop. O seletor interno continua disponível no Windows e no Linux com X11. O áudio do computador é desativado no Linux; a captura WASAPI com exclusão do Discord continua disponível no Windows.

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

O app prioriza H.264 High quando o Chromium anuncia suporte a esse perfil, preservando os demais para compatibilidade com o servidor. Em um teste no Windows com RX 6600, Baseline, Main e High usaram `MediaFoundationVideoEncodeAccelerator (AMDh264Encoder)`, enquanto Constrained Baseline usou `OpenH264`. O painel "Seu envio" mostra o perfil negociado e o encoder realmente usado. O LiveKit está fixado na versão 2.23.0 porque a configuração de preferência usa seu evento de criação do sender e o acesso ao transceiver do transporte.

No Linux, `VaapiVideoEncodeAccelerator` identifica a codificação por VA-API; `OpenH264` identifica software. O app habilita o encoder VA-API do Chromium e usa ANGLE/OpenGL por padrão. O driver precisa oferecer codificação para o codec escolhido; confira o encoder realmente usado no painel "Seu envio". É possível passar `--disable-features=AcceleratedVideoEncoder` para desativar essa opção ao diagnosticar problemas.

Se aparecer "processador", teste desligar o simulcast nas configurações e compartilhar de novo. Em Configurações, o estado de `Video Encode` mostra a disponibilidade geral da GPU; "Abrir diagnóstico da GPU" exibe os detalhes de `chrome://gpu`. A disponibilidade geral não garante que o perfil negociado ou a configuração de transmissão possam usar hardware.

## Estrutura

- `main.js`: janela, lista de telas, captura e tokens
- `video-runtime.js`: inicialização da codificação VA-API no Linux
- `test/`: testes da configuração de vídeo
- `.github/workflows/release.yml`: pacotes Windows e Linux para Releases
- `token.js`: gera ou busca o token da sala
- `preload.js`: ponte segura entre a interface e o `main.js`
- `renderer/`: interface (`index.html`, `style.css`, `js/`, `fonts/`)
- `server/`: servidor de tokens
- `build/`: ícone do app
