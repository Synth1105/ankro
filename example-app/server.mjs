import http from 'node:http';
import { readFile } from 'node:fs/promises';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

import { formatRequest, sendToAnkro } from './ankro-client.mjs';

const __filename = fileURLToPath(import.meta.url);
const __dirname = path.dirname(__filename);

const PORT = Number.parseInt(process.env.EXAMPLE_APP_PORT ?? '5555', 10);
const ANKRO_HOST = process.env.ANKRO_HOST ?? '127.0.0.1';
const ANKRO_PORT = Number.parseInt(process.env.ANKRO_PORT ?? '1234', 10);

const state = {
    pending: 0,
    completed: 0,
    failed: 0,
    attackActive: false,
    attackPaceMs: 180,
    burstSize: 8,
    recent: [],
    sequence: 1,
};

let attackTimer = null;

function pushEvent(kind, title, detail) {
    state.recent.unshift({
        kind,
        title,
        detail,
        timestamp: new Date().toLocaleTimeString([], {
            hour: '2-digit',
            minute: '2-digit',
            second: '2-digit',
        }),
    });

    state.recent.splice(20);
}

function snapshot() {
    return {
        ...state,
        queueMode: state.pending > 0 || state.attackActive,
        pressure: Math.min(100, Math.round((state.pending / Math.max(1, state.burstSize * 2)) * 100)),
        ankro: {
            host: ANKRO_HOST,
            port: ANKRO_PORT,
        },
    };
}

async function dispatchRequest({ source, payload }) {
    const id = `${source}-${state.sequence++}`;
    const startedAt = Date.now();
    state.pending += 1;

    pushEvent('queued', 'sent to ankro', `${id} is waiting for shield output`);

    try {
        const response = await sendToAnkro({
            host: ANKRO_HOST,
            port: ANKRO_PORT,
            lines: formatRequest({
                id,
                source,
                payload,
            }),
        });

        const durationMs = Date.now() - startedAt;
        state.completed += 1;
        pushEvent('direct', 'response received', `${id} completed in ${durationMs}ms`);

        return {
            id,
            ok: true,
            durationMs,
            response: response.trim(),
        };
    } catch (error) {
        state.failed += 1;
        pushEvent('alert', 'request failed', `${id} failed: ${error.message}`);
        return {
            id,
            ok: false,
            error: error.message,
        };
    } finally {
        state.pending = Math.max(0, state.pending - 1);
    }
}

async function sendBurst(size) {
    const requests = Array.from({ length: size }, (_, index) =>
        dispatchRequest({
            source: 'burst',
            payload: `burst-${index + 1}`,
        }),
    );

    return Promise.all(requests);
}

function startAttack() {
    if (attackTimer) {
        return;
    }

    state.attackActive = true;
    pushEvent('alert', 'attack started', 'bridge is flooding ankro with requests');
    attackTimer = setInterval(() => {
        void sendBurst(state.burstSize);
    }, state.attackPaceMs);
}

function stopAttack() {
    if (attackTimer) {
        clearInterval(attackTimer);
        attackTimer = null;
    }

    state.attackActive = false;
    pushEvent('drain', 'attack stopped', 'bridge is idle again');
}

function json(res, statusCode, body) {
    const payload = JSON.stringify(body);
    res.writeHead(statusCode, {
        'content-type': 'application/json; charset=utf-8',
        'content-length': Buffer.byteLength(payload),
    });
    res.end(payload);
}

function readBody(req) {
    return new Promise((resolve, reject) => {
        const chunks = [];
        req.on('data', (chunk) => chunks.push(chunk));
        req.on('end', () => {
            const raw = Buffer.concat(chunks).toString('utf8');
            if (!raw) {
                resolve({});
                return;
            }

            try {
                resolve(JSON.parse(raw));
            } catch (error) {
                reject(error);
            }
        });
        req.on('error', reject);
    });
}

async function serveStatic(urlPath, res) {
    const fileMap = new Map([
        ['/', 'index.html'],
        ['/index.html', 'index.html'],
        ['/styles.css', 'styles.css'],
        ['/app.mjs', 'app.mjs'],
        ['/ankro-client.mjs', 'ankro-client.mjs'],
    ]);

    const fileName = fileMap.get(urlPath);
    if (!fileName) {
        return false;
    }

    const filePath = path.join(__dirname, fileName);
    const content = await readFile(filePath);
    const contentType = fileName.endsWith('.css')
        ? 'text/css; charset=utf-8'
        : fileName.endsWith('.mjs')
            ? 'text/javascript; charset=utf-8'
            : 'text/html; charset=utf-8';

    res.writeHead(200, {
        'content-type': contentType,
        'content-length': content.length,
    });
    res.end(content);
    return true;
}

const server = http.createServer(async (req, res) => {
    try {
        const url = new URL(req.url, 'http://127.0.0.1');

        if (req.method === 'GET' && url.pathname === '/api/state') {
            json(res, 200, snapshot());
            return;
        }

        if (req.method === 'POST' && url.pathname === '/api/request') {
            const body = await readBody(req);
            const result = await dispatchRequest({
                source: body.source ?? 'probe',
                payload: body.payload ?? 'probe',
            });

            json(res, result.ok ? 200 : 502, {
                ...result,
                state: snapshot(),
            });
            return;
        }

        if (req.method === 'POST' && url.pathname === '/api/burst') {
            const body = await readBody(req);
            const size = Math.max(1, Number.parseInt(body.size ?? `${state.burstSize}`, 10));
            const results = await sendBurst(size);

            json(res, 200, {
                results,
                state: snapshot(),
            });
            return;
        }

        if (req.method === 'POST' && url.pathname === '/api/attack/start') {
            const body = await readBody(req);
            state.burstSize = Math.max(1, Number.parseInt(body.burstSize ?? `${state.burstSize}`, 10));
            state.attackPaceMs = Math.max(50, Number.parseInt(body.attackPaceMs ?? `${state.attackPaceMs}`, 10));
            startAttack();

            json(res, 200, { ok: true, state: snapshot() });
            return;
        }

        if (req.method === 'POST' && url.pathname === '/api/attack/stop') {
            stopAttack();
            json(res, 200, { ok: true, state: snapshot() });
            return;
        }

        if (await serveStatic(url.pathname, res)) {
            return;
        }

        res.writeHead(404, { 'content-type': 'text/plain; charset=utf-8' });
        res.end('not found');
    } catch (error) {
        json(res, 500, {
            ok: false,
            error: error.message,
        });
    }
});

server.listen(PORT, () => {
    process.stdout.write(`example-app bridge listening on http://127.0.0.1:${PORT}\n`);
    process.stdout.write(`using ankro at ${ANKRO_HOST}:${ANKRO_PORT}\n`);
});
