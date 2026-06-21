import net from 'node:net';

export async function sendToAnkro({
    host = '127.0.0.1',
    port = 1234,
    lines = [],
    timeoutMs = null,
    socketFactory = net.createConnection,
}) {
    return new Promise((resolve, reject) => {
        const socket = socketFactory({ host, port });
        const chunks = [];
        const shouldTimeout = Number.isFinite(timeoutMs) && timeoutMs > 0;
        const timer = shouldTimeout
            ? setTimeout(() => {
                socket.destroy();
                reject(new Error(`ankro request timed out after ${timeoutMs}ms`));
            }, timeoutMs)
            : null;

        socket.setEncoding('utf8');
        socket.on('data', (chunk) => chunks.push(chunk));
        socket.on('end', () => {
            if (timer) {
                clearTimeout(timer);
            }
            resolve(chunks.join(''));
        });
        socket.on('error', (err) => {
            if (timer) {
                clearTimeout(timer);
            }
            reject(err);
        });
        socket.on('connect', () => {
            socket.write(`${lines.join('\n')}\n\n`);
            socket.end();
        });
    });
}

export function formatRequest({
    id,
    source,
    payload,
    createdAt = new Date().toISOString(),
}) {
    return [
        `id=${id}`,
        `source=${source}`,
        `payload=${payload}`,
        `createdAt=${createdAt}`,
    ];
}
