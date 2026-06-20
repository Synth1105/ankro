import { MockDdosShield, createRequest } from './sim.mjs';

const model = new MockDdosShield();
const feed = [];
let requestSeq = 1;
let attackTimer = null;
let drainTimer = null;

const $ = (selector) => document.querySelector(selector);

const nodes = {
    modePill: $('#modePill'),
    modeText: $('#modeText'),
    attackToggle: $('#attackToggle'),
    probeBtn: $('#probeBtn'),
    burstBtn: $('#burstBtn'),
    burstSize: $('#burstSize'),
    burstSizeLabel: $('#burstSizeLabel'),
    attackPace: $('#attackPace'),
    attackPaceLabel: $('#attackPaceLabel'),
    busyValue: $('#busyValue'),
    queueModeValue: $('#queueModeValue'),
    queueLengthValue: $('#queueLengthValue'),
    directHandledValue: $('#directHandledValue'),
    drainedValue: $('#drainedValue'),
    queueRail: $('#queueRail'),
    eventFeed: $('#eventFeed'),
    pressureBadge: $('#pressureBadge'),
};

function nextId(prefix) {
    return `${prefix}-${requestSeq++}`;
}

function pushFeed(tone, title, detail) {
    feed.unshift({
        tone,
        title,
        detail,
        timestamp: new Date().toLocaleTimeString([], {
            hour: '2-digit',
            minute: '2-digit',
            second: '2-digit',
        }),
    });

    feed.splice(10);
}

function makeRequest(source) {
    return createRequest(nextId(source), source, `${source} request`);
}

function ingestRequest(source) {
    const request = makeRequest(source);
    const result = model.ingest(request);

    if (result.kind === 'direct') {
        pushFeed('direct', 'direct handled', `${request.id} went straight through`);
    } else {
        pushFeed('queued', 'queued', `${request.id} entered the backlog`);
    }

    render();
}

function fireBurst(count) {
    for (let index = 0; index < count; index += 1) {
        ingestRequest('burst');
    }
}

function startAttack() {
    if (attackTimer) {
        return;
    }

    model.setBusy(true);
    pushFeed('alert', 'mock DDoS started', 'shield switched to busy mode');
    render();

    const pace = Number(nodes.attackPace.value);
    attackTimer = window.setInterval(() => {
        fireBurst(Number(nodes.burstSize.value));
    }, pace);
    nodes.attackToggle.textContent = 'Stop mock DDoS';

    ensureDrainLoop();
}

function stopAttack() {
    if (attackTimer) {
        window.clearInterval(attackTimer);
        attackTimer = null;
    }

    model.setBusy(false);
    pushFeed('drain', 'mock DDoS stopped', 'consumer may drain the queue now');
    render();
    nodes.attackToggle.textContent = 'Start mock DDoS';
}

function ensureDrainLoop() {
    if (drainTimer) {
        return;
    }

    drainTimer = window.setInterval(() => {
        if (model.busy) {
            render();
            return;
        }

        const request = model.drainOne();
        if (!request) {
            render();
            return;
        }

        pushFeed('drain', 'consumed', `${request.id} left the queue`);
        render();
    }, 220);
}

function renderQueue() {
    const queue = model.queue;

    if (queue.length === 0) {
        nodes.queueRail.innerHTML = '<div class="queue-empty">No backlog. New requests flow directly.</div>';
        return;
    }

    nodes.queueRail.innerHTML = queue
        .map((request, index) => `
            <div class="request-chip">
                <b>${String(index + 1).padStart(2, '0')}. ${request.id}</b>
                <span>${request.source}</span>
            </div>
        `)
        .join('');
}

function renderFeed() {
    nodes.eventFeed.innerHTML = feed
        .map((entry) => `
            <li class="tone-${entry.tone}">
                <span class="meta">${entry.timestamp}</span>
                <strong>${entry.title}</strong>
                <div>${entry.detail}</div>
            </li>
        `)
        .join('');
}

function render() {
    const snapshot = model.snapshot();

    nodes.modeText.textContent = snapshot.queueMode ? 'queue mode' : 'direct mode';
    nodes.modePill.style.borderColor = snapshot.queueMode ? 'rgba(255, 181, 107, 0.5)' : 'rgba(115, 244, 199, 0.35)';
    nodes.modePill.style.background = snapshot.queueMode ? 'rgba(255, 181, 107, 0.1)' : 'rgba(115, 244, 199, 0.08)';
    nodes.busyValue.textContent = String(snapshot.busy);
    nodes.queueModeValue.textContent = String(snapshot.queueMode);
    nodes.queueLengthValue.textContent = String(snapshot.queueLength);
    nodes.directHandledValue.textContent = String(snapshot.directHandled);
    nodes.drainedValue.textContent = String(snapshot.drained);
    const pressure = Math.min(100, Math.round((snapshot.queueLength / snapshot.capacity) * 100));
    nodes.pressureBadge.textContent = `pressure ${pressure}%`;
    nodes.burstSizeLabel.textContent = nodes.burstSize.value;
    nodes.attackPaceLabel.textContent = `${nodes.attackPace.value}ms`;

    renderQueue();
    renderFeed();
}

nodes.attackToggle.addEventListener('click', () => {
    if (attackTimer) {
        stopAttack();
    } else {
        startAttack();
    }
});

nodes.probeBtn.addEventListener('click', () => {
    ingestRequest('probe');
});

nodes.burstBtn.addEventListener('click', () => {
    fireBurst(Number(nodes.burstSize.value));
});

nodes.burstSize.addEventListener('input', render);
nodes.attackPace.addEventListener('input', () => {
    if (attackTimer) {
        stopAttack();
        startAttack();
    }
    render();
});

ensureDrainLoop();
pushFeed('direct', 'shield online', 'idle traffic routes directly until the shield becomes busy');
render();
