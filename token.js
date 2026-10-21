// Busca o token de acesso à sala no servidor configurado.
// Fica separado do main.js pra poder ser testado sem abrir o Electron.
const ROOM_RE = /^[\p{L}\p{N}_\- ]{1,64}$/u;

function checkInput({ room, identity, name }) {
  if (typeof room !== 'string' || !ROOM_RE.test(room)) {
    throw new Error('Nome de sala inválido. Use letras, números, espaço, - e _ (até 64 caracteres).');
  }
  if (typeof identity !== 'string' || identity.length < 1 || identity.length > 64) {
    throw new Error('Identidade inválida.');
  }
  if (name != null && (typeof name !== 'string' || name.length > 40)) {
    throw new Error('Nome inválido (máximo de 40 caracteres).');
  }
}

function normalizeServerUrl(raw) {
  let u;
  try {
    u = new URL(String(raw || '').trim());
  } catch (_) {
    throw new Error('A URL do servidor de tokens é inválida. Exemplo: https://tokens.seudominio.com');
  }
  if (u.protocol !== 'http:' && u.protocol !== 'https:') {
    throw new Error('A URL do servidor de tokens deve começar com http:// ou https://');
  }
  return u.origin + u.pathname.replace(/\/+$/, '');
}

async function request(url, options) {
  try {
    return await fetch(url, { ...options, signal: AbortSignal.timeout(8000) });
  } catch (err) {
    if (err && err.name === 'TimeoutError') {
      throw new Error('O servidor de tokens demorou demais para responder.');
    }
    throw new Error('Não foi possível falar com o servidor de tokens. Confira a URL e a conexão.');
  }
}

// O servidor (pasta server/) guarda a chave e entrega o token.
async function fromServer({ room, identity, name, serverUrl, accessCode }) {
  checkInput({ room, identity, name });
  const base = normalizeServerUrl(serverUrl);
  const res = await request(`${base}/token`, {
    method: 'POST',
    headers: { 'content-type': 'application/json' },
    body: JSON.stringify({ room, identity, name, accessCode: accessCode || '' }),
  });
  let data = null;
  try {
    data = await res.json();
  } catch (_) { /* resposta sem JSON */ }

  if (res.status === 401) throw new Error('Código de acesso incorreto.');
  if (res.status === 429) throw new Error('Muitas tentativas seguidas. Espere um minuto e tente de novo.');
  if (!res.ok) throw new Error((data && data.error) || `O servidor de tokens respondeu com erro ${res.status}.`);
  if (!data || !data.token || !data.url) throw new Error('Resposta inesperada do servidor de tokens.');
  return { token: data.token, url: data.url };
}

async function checkServer(serverUrl) {
  const base = normalizeServerUrl(serverUrl);
  const res = await request(`${base}/health`, { method: 'GET' });
  if (!res.ok) throw new Error(`O servidor respondeu com erro ${res.status}.`);
  return true;
}

module.exports = { fromServer, checkServer, normalizeServerUrl };
