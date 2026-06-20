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

let latestState = {
    pending: 0,
    completed: 0,
    failed: 0,
    attackActive: false,
    attackPaceMs: Number(nodes.attackPace.value),
    burstSize: Number(nodes.burstSize.value),
    recent: [],
    queueMode: false,
    pressure: 0,
    ankro: {
        host: '127.0.0.1',
        port: 1234,
    },
};

function pushFeed(kind, title, detail) {
    latestState.recent = [
        {
            kind,
            title,
            detail,
            timestamp: new Date().toLocaleTimeString([], {
                hour: '2-digit',
                minute: '2-digit',
                second: '2-digit',
            }),
        },
        ...latestState.recent,
    ].slice(0, 20);
}

async function fetchState() {
    const response = await fetch('/api/state');
    if (!response.ok) {
        throw new Error(`state fetch failed: ${response.status}`);
    }

    latestState = await response.json();
    render();
}

async function postJson(url, body) {
    const response = await fetch(url, {
        method: 'POST',
        headers: {
            'content-type': 'application/json',
        },
        body: JSON.stringify(body),
    });

    const payload = await response.json();
    latestState = payload.state ?? latestState;
    render();

    if (!response.ok) {
        throw new Error(payload.error ?? `request failed: ${response.status}`);
    }

    return payload;
}

function renderQueue() {
    const queue = latestState.recent.filter((entry) => entry.kind === 'queued');

    if (latestState.pending === 0) {
        nodes.queueRail.innerHTML = '<div class="queue-empty">No pending requests. Ankro is draining directly.</div>';
        return;
    }

    nodes.queueRail.innerHTML = queue
        .slice(0, 8)
        .map(
            (request, index) => `
            <div class="request-chip">
                <b>${String(index + 1).padStart(2, '0')}. ${request.title}</b>
                <span>${request.detail}</span>
            </div>
        `,
        )
        .join('') || '<div class="queue-empty">Pending traffic is flowing through ankro.</div>';
}

function renderFeed() {
    nodes.eventFeed.innerHTML = latestState.recent
        .map(
            (entry) => `
            <li class="tone-${entry.kind}">
                <span class="meta">${entry.timestamp}</span>
                <strong>${entry.title}</strong>
                <div>${entry.detail}</div>
            </li>
        `,
        )
        .join('');
}

function render() {
    nodes.modeText.textContent = latestState.queueMode ? 'queue mode' : 'direct mode';
    nodes.modePill.style.borderColor = latestState.queueMode ? 'rgba(255, 181, 107, 0.5)' : 'rgba(115, 244, 199, 0.35)';
    nodes.modePill.style.background = latestState.queueMode ? 'rgba(255, 181, 107, 0.1)' : 'rgba(115, 244, 199, 0.08)';
    nodes.busyValue.textContent = String(Boolean(latestState.pending));
    nodes.queueModeValue.textContent = String(latestState.queueMode);
    nodes.queueLengthValue.textContent = String(latestState.pending);
    nodes.directHandledValue.textContent = String(latestState.completed);
    nodes.drainedValue.textContent = String(latestState.completed);
    nodes.pressureBadge.textContent = `pressure ${latestState.pressure}%`;
    nodes.burstSizeLabel.textContent = String(latestState.burstSize);
    nodes.attackPaceLabel.textContent = `${latestState.attackPaceMs}ms`;
    nodes.attackToggle.textContent = latestState.attackActive ? 'Stop DDoS' : 'Start DDoS';

    renderQueue();
    renderFeed();
}

nodes.attackToggle.addEventListener('click', async () => {
    try {
        if (latestState.attackActive) {
            await postJson('/api/attack/stop', {});
        } else {
            await postJson('/api/attack/start', {
                burstSize: Number(nodes.burstSize.value),
                attackPaceMs: Number(nodes.attackPace.value),
            });
        }
    } catch (error) {
        pushFeed('alert', 'attack control failed', error.message);
        render();
    }
});

nodes.probeBtn.addEventListener('click', async () => {
    try {
        await postJson('/api/request', {
            source: 'probe',
            payload: 'probe request',
        });
    } catch (error) {
        pushFeed('alert', 'probe failed', error.message);
        render();
    }
});

nodes.burstBtn.addEventListener('click', async () => {
    try {
        await postJson('/api/burst', {
            size: Number(nodes.burstSize.value),
        });
    } catch (error) {
        pushFeed('alert', 'burst failed', error.message);
        render();
    }
});

nodes.burstSize.addEventListener('input', render);
nodes.attackPace.addEventListener('input', render);

setInterval(() => {
    void fetchState().catch((error) => {
        pushFeed('alert', 'state refresh failed', error.message);
        render();
    });
}, 500);

pushFeed('direct', 'bridge ready', 'example-app is connected to the ankro bridge');
render();
void fetchState().catch((error) => {
    pushFeed('alert', 'initial state fetch failed', error.message);
    render();
});
