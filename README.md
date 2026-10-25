# getScreenShare

Aplicativo de desktop para compartilhar telas, janelas e câmera em salas, feito com Electron e WebRTC/LiveKit. A latência e a qualidade dependem da rede, do dispositivo e do codec.

## O que preciso instalar?

| Objetivo | Requisitos |
| --- | --- |
| Usar o instalador ou AppImage | Pacote da [release](https://github.com/gwennocas/getScreenShare/releases), dependências do sistema descritas abaixo e URL/código de um servidor de tokens existente. O runtime Node.js vem dentro do Electron. |
| Rodar ou empacotar o código do app | Git, Node.js e npm, `npm ci` na raiz e as mesmas dependências do sistema usadas pelo aplicativo. |
| Hospedar o servidor de tokens | Node.js/npm, instalação separada em `server/`, configuração de `server/.env` e acesso a um servidor LiveKit. |

Para desenvolvimento e hospedagem, use **Node.js 24 LTS**, a versão usada no workflow de release. O mínimo declarado pelo projeto é **Node.js 22.12.0**, exigido pelo Electron fixado no lockfile. O npm vem com a distribuição oficial do Node. O arquivo `.nvmrc` seleciona a versão 24 para quem usa nvm.

O aplicativo instalado se conecta a um **servidor de tokens**, que autoriza a entrada na sala; o áudio/vídeo é transportado pelo **servidor LiveKit**. Quem já recebeu uma URL e um código de acesso pode usar essa infraestrutura existente. Para hospedar a própria, veja [Servidor de tokens e LiveKit](#servidor-de-tokens-e-livekit).

## Plataformas e recursos

| Recurso | Windows x64 | Linux x64 |
| --- | --- | --- |
| Tela/janela | Seletor do aplicativo | Portal do sistema no Wayland; seletor interno no X11 |
| Câmera | Disponível | Disponível |
| GPU para codificação | Conforme codec e driver | VA-API, conforme codec e driver |
| Áudio do computador excluindo Discord | WASAPI, com os requisitos abaixo | Indisponível nesta versão |
| Atualização pelo app | Disponível no instalador NSIS | Instalação manual do novo AppImage |

A captura Wayland e a codificação por GPU foram confirmadas em **Arch Linux, Hyprland e Radeon RX 6600**. As releases publicam pacotes x64 para Linux e Windows. GNOME, KDE e X11 precisam de validação nos respectivos ambientes; a compilação por CI não substitui esse teste de desktop.

## Usar uma versão pronta

Baixe o pacote na página de [Releases](https://github.com/gwennocas/getScreenShare/releases):

- **Windows:** execute `getScreenShare-Setup-VERSÃO.exe` em um Windows 10/11 x64 compatível com Electron 44. Para o áudio filtrado, use Windows 10 versão 2004 ou mais recente, com Discord aberto.
- **Linux:** baixe `getScreenShare-VERSÃO-linux-x86_64.AppImage`, permita sua execução e abra o arquivo. Exemplo para a versão 0.2.7:

  ```sh
  chmod +x getScreenShare-0.2.7-linux-x86_64.AppImage
  ./getScreenShare-0.2.7-linux-x86_64.AppImage
  ```

Configure a URL base do servidor de tokens e o código em **Configurações → Geral → Conexão**. Entre em uma sala pelo nome e compartilhe esse mesmo nome com os outros participantes.

**Ligar câmera** permite escolher e publicar uma câmera junto com a tela. Use **Detectar outras câmeras** para autorizar o acesso e atualizar a lista de dispositivos. O microfone não é ativado pelo aplicativo.

### Dependências do sistema no Linux

O AppImage inclui o aplicativo e o Electron. A sessão gráfica, os serviços de captura e os drivers são fornecidos pela distribuição.

| Componente | Quando é necessário |
| --- | --- |
| Bibliotecas de desktop do Electron, como GTK 3, NSS, ALSA, AT-SPI e GBM | Para abrir o aplicativo. Normalmente já estão presentes em uma instalação de desktop; sistemas mínimos podem precisar dos pacotes correspondentes da distribuição. |
| PipeWire e um gerenciador de sessão, como WirePlumber | Para compartilhar a tela no Wayland. |
| `xdg-desktop-portal` e backend do desktop | Para abrir o seletor e autorizar a captura Wayland. |
| FUSE 2, inclusive `libfuse.so.2` | Para montar o AppImage quando a distribuição não oferece essa compatibilidade por padrão. |
| Driver VA-API da GPU | Para tentar codificação por hardware. O app pode usar software quando o codec/driver não permite aceleração. |

Instale o backend adequado ao seu desktop:

| Desktop | Backend de captura |
| --- | --- |
| Hyprland | `xdg-desktop-portal-hyprland` |
| GNOME | `xdg-desktop-portal-gnome` |
| KDE Plasma | `xdg-desktop-portal-kde` |
| Outros compositores | Consulte o backend recomendado pelo compositor. |

Mantenha o backend de seleção de arquivos usado pelo desktop. O backend GTK complementa o Hyprland e não implementa sozinho a captura de tela.

**Exemplo Arch Linux + Hyprland**, para componentes que ainda não estejam instalados:

```sh
sudo pacman -S --needed pipewire wireplumber xdg-desktop-portal \
  xdg-desktop-portal-hyprland xdg-desktop-portal-gtk fuse2
```

**Exemplo Ubuntu/Debian com GNOME:**

```sh
sudo apt install pipewire wireplumber xdg-desktop-portal xdg-desktop-portal-gnome
```

Para o AppImage, Ubuntu 24.04 usa `sudo apt install libfuse2t64`; Ubuntu 22.04 e versões Debian que oferecem esse nome usam `sudo apt install libfuse2`. Confira o nome disponível na sua distribuição e mantenha o FUSE 3 do desktop. Veja a [documentação do AppImage](https://docs.appimage.org/user-guide/troubleshooting/fuse.html).

No Arch, o driver VA-API AMD é fornecido pelo Mesa; `libva-utils` oferece o diagnóstico `vainfo`. Para Intel ou NVIDIA, consulte o driver compatível com sua GPU: suporte a decodificação não garante suporte a codificação. Veja [aceleração de vídeo no Arch](https://wiki.archlinux.org/title/Hardware_video_acceleration).

Depois de instalar um backend de portal com a sessão já aberta, encerre e abra a sessão ou reinicie seus serviços de portal. No Hyprland:

```sh
systemctl --user restart xdg-desktop-portal-hyprland.service
systemctl --user restart xdg-desktop-portal.service
```

## Rodar a partir do código

1. Instale [Node.js 24 LTS](https://nodejs.org/) com npm e [Git](https://git-scm.com/).
2. Instale as dependências do sistema correspondentes ao seu desktop.
3. No terminal:

   ```sh
   git clone https://github.com/gwennocas/getScreenShare.git
   cd getScreenShare
   npm ci
   npm test
   npm start
   ```

4. Configure a URL/código do servidor de tokens em **Configurações → Geral → Conexão** e clique em **Testar servidor**.

`npm ci` instala as versões do `package-lock.json`. As dependências de desenvolvimento também são necessárias para iniciar o Electron e gerar os pacotes; evite `--omit=dev` nesse fluxo. O primeiro início ou empacotamento pode baixar o binário do Electron e ferramentas de build do GitHub, além dos pacotes do registro npm.

Para testar duas pessoas no mesmo computador, abra dois terminais com `npm start` e entre na mesma sala. No Windows, deixe o áudio do computador desmarcado nesse teste para evitar retorno do som.

### Dependências JavaScript

| Pacote | Função | Instalação |
| --- | --- | --- |
| `electron` | Runtime de desktop, captura, permissões e APIs nativas | Raiz, dependência de desenvolvimento |
| `electron-builder` | AppImage, NSIS e outros alvos de empacotamento | Raiz, dependência de desenvolvimento |
| `livekit-client` | Salas, publicação/recepção de mídia e conexão WebRTC | Raiz |
| `electron-updater` | Atualizações do aplicativo Windows | Raiz |
| `wasapi-loopback` | Captura nativa de áudio excluindo o Discord | Opcional na raiz; somente Windows x64 |
| `livekit-server-sdk` e `dotenv` | Emissão de tokens e leitura do `.env` | Instalação separada em `server/` |

O `wasapi-loopback` vem de um commit fixado no GitHub e usa um binário `.node` pré-compilado. Os scripts atuais empacotam esse binário; recompilar o addon nativo é um processo separado. Como a dependência é opcional, confira sua presença no Windows antes de distribuir uma build com áudio:

```sh
node -e "require('node:fs').accessSync('node_modules/wasapi-loopback/prebuilds/windows-x86_64/wasapi-loopback-win32-x64.node')"
```

O workflow de release faz essa verificação. O LiveKit está fixado em 2.23.0 porque a preferência de perfil H.264 usa um evento do SDK e o acesso ao transporte interno em `renderer/js/encoder.js`; atualizações precisam ser testadas nessa integração.

## Gerar os pacotes

Execute os comandos a partir da raiz, depois de `npm ci` e `npm test`:

| Comando | Resultado |
| --- | --- |
| `npm run dist` | Alvo padrão do sistema atual: AppImage no Linux ou NSIS no Windows |
| `npm run dist:linux` | AppImage Linux |
| `npm run dist:win` | Instalador Windows NSIS |
| `npm run dist:deb` | Pacote `.deb`, como alvo adicional para compilação local |

Os arquivos ficam em `dist/`. O caminho validado pelo CI compila **Linux no Ubuntu** e **Windows no Windows**. Para reproduzi-lo, use o sistema de destino; compilação cruzada pode exigir ferramentas adicionais, como Wine para determinados alvos Windows.

Se o alvo `.deb` falhar por falta de `libcrypt.so.1`, instale a biblioteca de compatibilidade da distribuição de build. No Arch, ela é fornecida por [`libxcrypt-compat`](https://archlinux.org/packages/core/x86_64/libxcrypt-compat/). Essa exigência vem da ferramenta de empacotamento; as releases atuais distribuem AppImage e NSIS.

## Servidor de tokens e LiveKit

A raiz e `server/` têm lockfiles e instalações independentes. Para instalar o servidor, a partir da raiz:

```sh
npm --prefix server ci
```

Copie `server/.env.example` para `server/.env` (`cp` no Linux ou `Copy-Item` no PowerShell) e preencha:

| Variável | Valor |
| --- | --- |
| `LIVEKIT_URL` | URL WebSocket do seu servidor LiveKit, normalmente `wss://...` |
| `LIVEKIT_API_KEY` / `LIVEKIT_API_SECRET` | Credenciais do servidor LiveKit |
| `ACCESS_CODE` | Código compartilhado, longo e difícil de adivinhar |
| `PORT` | Porta HTTP do servidor de tokens; padrão 8787 |
| `TRUST_PROXY` | Padrão 0; use 1 somente atrás de um proxy confiável que controle `X-Forwarded-For` |

Inicie pelo diretório do servidor, para carregar `server/.env`:

```sh
npm --prefix server start
```

`npm run server`, na raiz, é um atalho para esse mesmo comando.

Teste `http://localhost:8787/health`; a resposta esperada é `{"ok":true}`. Essa rota verifica o servidor de tokens, sem testar uma conexão de mídia com o LiveKit.

O servidor LiveKit pode ser o [LiveKit Cloud](https://cloud.livekit.io/) ou uma [instância hospedada por você](https://docs.livekit.io/transport/self-hosting/). O servidor de tokens apenas emite a autorização; ele não substitui o servidor de mídia.

As credenciais secretas ficam no servidor. O app recebe a URL base do serviço de tokens, sem adicionar `/token`, e o `ACCESS_CODE`. Para acesso pela internet, publique o serviço de tokens com HTTPS. Todas as pessoas com esse código podem solicitar acesso a salas da instância; o nome da sala não é uma senha individual. O código é salvo nas configurações locais do app (`localStorage`).

### Hospedar no Render

O arquivo `render.yaml` configura Root Directory `server`, Build Command `npm ci`, Start Command `npm start`, health check `/health` e `TRUST_PROXY=1`. No Render, crie um serviço por **New → Blueprint**, conecte o repositório e informe as quatro variáveis secretas solicitadas. Depois confira `/health` na URL pública e configure essa URL no aplicativo. Em planos com suspensão por inatividade, a primeira conexão pode demorar enquanto o serviço acorda.

## Atualizar e publicar

**Windows instalado:** use **Configurações → Atualizações**, verifique a versão, baixe e reinicie para instalar. **Linux:** baixe o novo AppImage e substitua o arquivo anterior. As configurações ficam no perfil do usuário.

Para publicar:

1. Atualize a versão de `package.json` e `package-lock.json`, execute `npm test` e envie as alterações ao repositório.
2. Crie uma release no GitHub com a tag `vVERSÃO`, apontando para o commit correspondente.
3. Ao publicar, `.github/workflows/release.yml` instala as dependências, executa os testes e gera os pacotes nos dois sistemas. O workflow usa o token automático do GitHub Actions para anexar os arquivos.
4. Aguarde as duas execuções concluírem com sucesso e confira AppImage, instalador, blockmap e metadados `latest*.yml` na release.

Para publicar localmente, configure `GH_TOKEN` no ambiente de build e use `npm run release` no sistema de destino. Credenciais de publicação pertencem ao ambiente de build.

## Diagnóstico

| Sintoma | O que conferir |
| --- | --- |
| AppImage pede FUSE | Instale a compatibilidade FUSE 2 correspondente à distribuição. |
| Arquivo `.so` ausente | Execute o AppImage pelo terminal e instale o pacote da biblioteca apontada pelo erro. Bibliotecas do desktop continuam sendo dependências do sistema. |
| Nenhum seletor Wayland, inclusive no navegador | Verifique PipeWire, WirePlumber, backend do portal e ambiente da sessão. |
| `No such interface org.freedesktop.portal.ScreenCast` | O portal principal não carregou a interface de captura; confira o backend e reinicie os serviços/sessão. |
| Tela funciona, áudio não inicia no Windows | Confira Windows x64 compatível, Discord aberto e o binário WASAPI. |
| `Codificação: processador` / `OpenH264` | Confira driver, codec e perfis de codificação. Experimente desativar simulcast e iniciar outra transmissão. |
| GPU disponível, mas envio sem frames/bitrate | O LiveKit pode suspender camadas sem espectadores; valide com outra pessoa na sala. |
| Servidor de tokens não inicia | Instale `server/` separadamente e confira `server/.env`, Node e as variáveis obrigatórias. |

No Hyprland, consulte:

```sh
journalctl --user -b -u xdg-desktop-portal -u xdg-desktop-portal-hyprland
systemctl --user show-environment
```

O ambiente dos serviços deve conter `XDG_CURRENT_DESKTOP` e `WAYLAND_DISPLAY`. Veja a [documentação de compartilhamento do Hyprland](https://wiki.hypr.land/useful-utilities/screen-sharing/).

O painel **Seu envio** mostra o encoder e o perfil negociado. No Linux, `VaapiVideoEncodeAccelerator` indica VA-API; `OpenH264` é software. **Abrir diagnóstico da GPU** mostra `chrome://gpu`; a disponibilidade geral de `Video Encode` não garante que todo perfil/configuração use hardware. O app habilita VA-API com ANGLE/OpenGL; `--disable-features=AcceleratedVideoEncoder` permite desativar essa opção para diagnóstico.

O app prioriza H.264 High quando o Chromium anuncia suporte. No teste Windows com RX 6600, Baseline, Main e High usaram `MediaFoundationVideoEncodeAccelerator (AMDh264Encoder)`; Constrained Baseline usou `OpenH264`.

## Desenvolvimento e validação

- `renderer/js/settings.js`: resolução, fps, bitrate, codec, simulcast e preferências locais.
- `renderer/js/app.js`: salas, câmera, captura, reprodução e interface.
- `renderer/js/stats.js` e `encoder.js`: estatísticas e preferência de perfil H.264.
- `main.js` / `preload.js`: janela, permissões e ponte entre interface e APIs nativas.
- `video-runtime.js` / `room-clipboard.js`: configuração VA-API e cópia nativa.
- `token.js`: cliente do servidor de tokens; `server/`: emissão de tokens.
- `test/`: testes de configuração de vídeo e clipboard.
- `build/`: ícones; `.github/workflows/release.yml`: publicação.

`npm test` cobre essas unidades, não o fluxo completo de mídia. Antes de distribuir mudanças de captura/SDK, teste entrada de duas pessoas, tela/janela, câmera, encerramento da captura e reconexão nos sistemas afetados. O workflow atual executa testes na publicação da release; ainda não há um workflow de testes por pull request.
