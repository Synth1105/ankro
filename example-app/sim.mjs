export class MockDdosShield {
    constructor({ capacity = 4 } = {}) {
        this.capacity = capacity;
        this.busy = false;
        this.queue = [];
        this.directHandled = [];
        this.drained = [];
    }

    get queueMode() {
        return this.busy || this.queue.length > 0;
    }

    setBusy(nextBusy) {
        this.busy = nextBusy;
    }

    ingest(request) {
        if (this.queueMode) {
            this.queue.push(request);
            return { kind: 'queued', request };
        }

        this.directHandled.push(request);
        return { kind: 'direct', request };
    }

    drainOne() {
        if (this.busy || this.queue.length === 0) {
            return null;
        }

        const request = this.queue.shift();
        this.drained.push(request);
        return request;
    }

    drainAll(limit = Infinity) {
        const drained = [];

        while (drained.length < limit) {
            const request = this.drainOne();
            if (!request) {
                break;
            }

            drained.push(request);
        }

        return drained;
    }

    snapshot() {
        return {
            capacity: this.capacity,
            busy: this.busy,
            queueMode: this.queueMode,
            queueLength: this.queue.length,
            directHandled: this.directHandled.length,
            drained: this.drained.length,
        };
    }
}

export function createRequest(id, source, payload = '') {
    return {
        id,
        source,
        payload,
        createdAt: new Date().toISOString(),
    };
}
