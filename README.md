# getScreenShare

App de desktop para compartilhar a tela em tempo real, feito com Electron e WebRTC (LiveKit).
Cada pessoa entra numa sala pelo nome, escolhe uma tela ou janela e todos na sala assistem com poucos milissegundos de atraso.

## Rodar durante o desenvolvimento (Windows)

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

No Windows, rode `npm run dist`. O instalador aparece na pasta `dist/`. Depois de instalar, configure a URL do servidor de tokens e o código de acesso. Nenhum arquivo `.env` é necessário no computador de quem usa o app.

### Atualizar o app

No app instalado, abra **Configurações → Atualizações** e toque em **Verificar atualizações**. Se houver uma versão nova, toque em **Baixar atualização**. Quando o download terminar, toque em **Reiniciar e instalar**. O app também verifica atualizações ao abrir, mas só baixa e instala depois desses cliques.

Para publicar uma versão, atualize o campo `version` em `package.json`, configure `GH_TOKEN` com permissão de escrita no repositório e rode `npm run release` no Windows. O comando gera o instalador e publica os arquivos necessários em uma GitHub Release. A versão `0.2.2` inclui o atualizador; quem usa uma versão anterior precisa instalar manualmente uma versão com o atualizador uma vez.

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

O app prioriza H.264 High quando o Chromium anuncia suporte a esse perfil, preservando os demais para compatibilidade com o servidor. Em um teste local com RX 6600, Baseline, Main e High usaram `MediaFoundationVideoEncodeAccelerator (AMDh264Encoder)`, enquanto Constrained Baseline usou `OpenH264`. O painel "Seu envio" mostra o perfil negociado e o encoder realmente usado. O LiveKit está fixado na versão 2.23.0 porque a configuração de preferência usa seu evento de criação do sender e o acesso ao transceiver do transporte.

Se aparecer "processador", teste desligar o simulcast nas configurações e compartilhar de novo. Em Configurações, o estado de `Video Encode` mostra a disponibilidade geral da GPU; "Abrir diagnóstico da GPU" exibe os detalhes de `chrome://gpu`. A disponibilidade geral não garante que o perfil negociado ou a configuração de transmissão possam usar hardware.

## Estrutura

- `main.js`: janela, lista de telas, captura e tokens
- `token.js`: gera ou busca o token da sala
- `preload.js`: ponte segura entre a interface e o `main.js`
- `renderer/`: interface (`index.html`, `style.css`, `js/`, `fonts/`)
- `server/`: servidor de tokens
- `build/`: ícone do app
