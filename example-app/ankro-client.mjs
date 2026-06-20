import net from 'node:net';

export async function sendToAnkro({
    host = '127.0.0.1',
    port = 1234,
    lines = [],
    timeoutMs = 10000,
    socketFactory = net.createConnection,
}) {
    return new Promise((resolve, reject) => {
        const socket = socketFactory({ host, port });
        const chunks = [];
        const timer = setTimeout(() => {
            socket.destroy();
            reject(new Error(`ankro request timed out after ${timeoutMs}ms`));
        }, timeoutMs);

        socket.setEncoding('utf8');
        socket.on('data', (chunk) => chunks.push(chunk));
        socket.on('end', () => {
            clearTimeout(timer);
            resolve(chunks.join(''));
        });
        socket.on('error', (err) => {
            clearTimeout(timer);
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
