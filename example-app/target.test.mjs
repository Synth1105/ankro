import test from 'node:test';
import assert from 'node:assert/strict';
import { execFile, spawn } from 'node:child_process';
import { promisify } from 'node:util';
import fs from 'node:fs';
import path from 'node:path';

const execFileAsync = promisify(execFile);
const lockPath = '/tmp/ankro-example-target.lock';
const targetPath = path.resolve('example-app/target');

function cleanupLock() {
    if (fs.existsSync(lockPath)) {
        fs.unlinkSync(lockPath);
    }
}

async function waitForBusy() {
    for (let attempt = 0; attempt < 50; attempt += 1) {
        const { stdout } = await execFileAsync(targetPath, ['-b'], {
            env: {
                ...process.env,
                ANKRO_EXAMPLE_LATENCY_MS: '250',
            },
        });

        if (stdout.includes('busy')) {
            return;
        }

        await new Promise((resolve) => setTimeout(resolve, 20));
    }

    throw new Error('target never reported busy');
}

test('target reports idle state with -b', async () => {
    cleanupLock();

    const { stdout } = await execFileAsync(targetPath, ['-b'], {
        env: {
            ...process.env,
            ANKRO_EXAMPLE_LATENCY_MS: '10',
        },
    });

    assert.equal(stdout, '');
});

test('target handles -r and exposes busy while working', async () => {
    cleanupLock();

    const worker = spawn(targetPath, ['-r', 'hello'], {
        env: {
            ...process.env,
            ANKRO_EXAMPLE_LATENCY_MS: '250',
        },
    });

    await waitForBusy();

    const output = await new Promise((resolve, reject) => {
        let stdout = '';
        let stderr = '';

        worker.stdout.on('data', (chunk) => {
            stdout += chunk.toString('utf8');
        });
        worker.stderr.on('data', (chunk) => {
            stderr += chunk.toString('utf8');
        });
        worker.on('error', reject);
        worker.on('close', (code) => {
            if (code !== 0) {
                reject(new Error(`target exited with ${code}: ${stderr}`));
                return;
            }

            resolve(stdout);
        });
    });

    assert.match(output, /ok hello/);
    cleanupLock();
});
