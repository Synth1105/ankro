import test from 'node:test';
import assert from 'node:assert/strict';

import { MockDdosShield } from './sim.mjs';

test('idle requests go directly through', () => {
    const model = new MockDdosShield();
    const request = { id: 'probe-1', source: 'probe' };

    const result = model.ingest(request);

    assert.equal(result.kind, 'direct');
    assert.equal(model.queue.length, 0);
    assert.equal(model.directHandled.length, 1);
    assert.equal(model.queueMode, false);
});

test('busy requests are queued and drained in order', () => {
    const model = new MockDdosShield();
    model.setBusy(true);

    const first = { id: 'burst-1', source: 'burst' };
    const second = { id: 'burst-2', source: 'burst' };

    assert.equal(model.ingest(first).kind, 'queued');
    assert.equal(model.ingest(second).kind, 'queued');
    assert.equal(model.queue.length, 2);
    assert.equal(model.queueMode, true);

    model.setBusy(false);

    const drainedFirst = model.drainOne();
    const drainedSecond = model.drainOne();

    assert.deepEqual(drainedFirst, first);
    assert.deepEqual(drainedSecond, second);
    assert.equal(model.queue.length, 0);
    assert.equal(model.queueMode, false);
    assert.equal(model.drained.length, 2);
});

test('direct mode returns after the queue is empty', () => {
    const model = new MockDdosShield();
    model.setBusy(true);

    model.ingest({ id: 'ddos-1', source: 'ddos' });
    model.ingest({ id: 'ddos-2', source: 'ddos' });

    model.setBusy(false);
    model.drainAll();

    assert.equal(model.queue.length, 0);
    assert.equal(model.queueMode, false);

    const direct = model.ingest({ id: 'probe-2', source: 'probe' });

    assert.equal(direct.kind, 'direct');
    assert.equal(model.directHandled.length, 1);
});
