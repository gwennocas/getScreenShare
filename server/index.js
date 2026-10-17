// Servidor de tokens do getScreenShare.
// Guarda a chave secreta do LiveKit e entrega tokens de sala para quem tiver o código de acesso.
require('dotenv').config({ quiet: true });
const http = require('http');
const crypto = require('crypto');
const { AccessToken } = require('livekit-server-sdk');

const { LIVEKIT_URL, LIVEKIT_API_KEY, LIVEKIT_API_SECRET, ACCESS_CODE } = process.env;
const PORT = Number(process.env.PORT) || 8787;
const TRUST_PROXY = process.env.TRUST_PROXY === '1';

if (!LIVEKIT_URL || !LIVEKIT_API_KEY || !LIVEKIT_API_SECRET) {
  console.error('Faltam LIVEKIT_URL, LIVEKIT_API_KEY e LIVEKIT_API_SECRET. Veja o arquivo .env.example.');
  process.exit(1);
}
if (!ACCESS_CODE) {
  console.error('Falta ACCESS_CODE. Defina um código de acesso antes de publicar o servidor.');
  process.exit(1);
}

const ROOM_RE = /^[\p{L}\p{N}_\- ]{1,64}$/u;

// Limite simples: 30 pedidos por minuto por endereço.
const hits = new Map();
function limited(ip) {
  const now = Date.now();
  const recent = (hits.get(ip) || []).filter((t) => now - t < 60_000);
  recent.push(now);
  hits.set(ip, recent);
  return recent.length > 30;
}
setInterval(() => {
  const now = Date.now();
  for (const [ip, list] of hits) {
    if (!list.some((t) => now - t < 60_000)) hits.delete(ip);
  }
}, 60_000).unref();

function sameCode(a, b) {
  const ha = crypto.createHash('sha256').update(String(a)).digest();
  const hb = crypto.createHash('sha256').update(String(b)).digest();
  return crypto.timingSafeEqual(ha, hb);
}

function send(res, status, body) {
  const data = JSON.stringify(body);
  res.writeHead(status, { 'content-type': 'application/json; charset=utf-8', 'cache-control': 'no-store' });
  res.end(data);
}

function readJson(req, limit = 4096) {
  return new Promise((resolve, reject) => {
    let size = 0;
    let tooBig = false;
    const chunks = [];
    req.on('data', (c) => {
      if (tooBig) return;
      size += c.length;
      if (size > limit) {
        tooBig = true;
        chunks.length = 0;
        reject(Object.assign(new Error('Pedido grande demais.'), { status: 413 }));
        return;
      }
      chunks.push(c);
    });
    req.on('end', () => {
      try {
        resolve(JSON.parse(Buffer.concat(chunks).toString('utf8') || '{}'));
      } catch (_) {
        reject(Object.assign(new Error('JSON inválido.'), { status: 400 }));
      }
    });
    req.on('error', reject);
  });
}

const server = http.createServer(async (req, res) => {
  const url = new URL(req.url, 'http://localhost');
  const forwarded = TRUST_PROXY && String(req.headers['x-forwarded-for'] || '').split(',')[0].trim();
  const ip = forwarded || req.socket.remoteAddress || 'desconhecido';

  if (req.method === 'GET' && url.pathname === '/health') return send(res, 200, { ok: true });

  if (req.method === 'POST' && url.pathname === '/token') {
    if (limited(ip)) return send(res, 429, { error: 'Muitas tentativas. Espere um minuto.' });
    try {
      const { room, identity, name, accessCode } = await readJson(req);

      if (!sameCode(accessCode || '', ACCESS_CODE)) {
        return send(res, 401, { error: 'Código de acesso incorreto.' });
      }
      if (typeof room !== 'string' || !ROOM_RE.test(room)) {
        return send(res, 400, { error: 'Nome de sala inválido.' });
      }
      if (typeof identity !== 'string' || identity.length < 1 || identity.length > 64) {
        return send(res, 400, { error: 'Identidade inválida.' });
      }
      if (name != null && (typeof name !== 'string' || name.length > 40)) {
        return send(res, 400, { error: 'Nome inválido.' });
      }

      const at = new AccessToken(LIVEKIT_API_KEY, LIVEKIT_API_SECRET, {
        identity,
        name: name || identity,
        ttl: '2h',
      });
      at.addGrant({ roomJoin: true, room, canPublish: true, canSubscribe: true });
      return send(res, 200, { token: await at.toJwt(), url: LIVEKIT_URL });
    } catch (err) {
      if (err.status === 413) {
        // responde e só então fecha a conexão, para não receber o resto do corpo
        res.setHeader('connection', 'close');
        res.on('finish', () => req.destroy());
      }
      if (err.status) return send(res, err.status, { error: err.message });
      console.error('Erro ao gerar token:', err);
      return send(res, 500, { error: 'Erro interno ao gerar o token.' });
    }
  }

  return send(res, 404, { error: 'Não encontrado.' });
});

server.listen(PORT, () => console.log(`Servidor de tokens ouvindo na porta ${PORT}`));
