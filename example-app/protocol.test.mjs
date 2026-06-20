import test from 'node:test';
import assert from 'node:assert/strict';
import { EventEmitter } from 'node:events';

import { formatRequest, sendToAnkro } from './ankro-client.mjs';

test('formatRequest produces ankro request lines', () => {
    assert.deepEqual(
        formatRequest({
            id: 'probe-1',
            source: 'probe',
            payload: 'hello',
            createdAt: '2026-06-20T00:00:00.000Z',
        }),
        [
            'id=probe-1',
            'source=probe',
            'payload=hello',
            'createdAt=2026-06-20T00:00:00.000Z',
        ],
    );
});

test('sendToAnkro writes payload and waits for response', async () => {
    let written = '';
    const socket = new EventEmitter();
    socket.setEncoding = () => {};
    socket.write = (chunk) => {
        written += chunk;
    };
    socket.end = () => {
        socket.emit('data', 'ok\n');
        socket.emit('end');
    };
    socket.destroy = () => {};

    queueMicrotask(() => {
        socket.emit('connect');
    });

    const response = await sendToAnkro({
        host: '127.0.0.1',
        port: 1234,
        lines: ['id=1', 'source=probe', 'payload=test'],
        timeoutMs: 2000,
        socketFactory: () => socket,
    });

    assert.equal(response, 'ok\n');
    assert.equal(written, 'id=1\nsource=probe\npayload=test\n\n');
});
