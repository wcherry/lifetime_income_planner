import * as fs from 'fs';
import * as path from 'path';

/** Kill a detached child and everything in its process group. Best-effort. */
function killGroup(pid: number): void {
  try {
    process.kill(-pid, 'SIGTERM');
  } catch {
    // Group already gone — try the bare pid, then give up.
    try {
      process.kill(pid, 'SIGTERM');
    } catch {
      // Already exited.
    }
  }
}

export default async function globalTeardown(): Promise<void> {
  const runDir = process.env.RUN_DIR;
  if (!runDir) {
    console.error('Teardown: RUN_DIR not set, nothing to stop.');
    return;
  }

  // SKIP_SERVERS runs never spawned anything, so there is nothing to tear down.
  const pidsFile = path.join(runDir, '.pids.json');
  if (!fs.existsSync(pidsFile)) {
    console.log('Teardown: no .pids.json — leaving any externally-started servers running.');
    console.log(`\nArtifacts saved to: ${runDir}`);
    return;
  }

  console.log('Stopping backend + frontend…');
  const { backendPid, frontendPid } = JSON.parse(fs.readFileSync(pidsFile, 'utf-8'));
  if (frontendPid) killGroup(frontendPid);
  if (backendPid) killGroup(backendPid);

  console.log(`\nArtifacts saved to: ${runDir}`);
  console.log(`  Service logs : ${runDir}/service-logs/`);
  console.log(`  Browser logs : ${runDir}/browser-logs/`);
  console.log(`  PW artifacts : ${runDir}/playwright-artifacts/`);
  console.log(`  PW report    : ${runDir}/playwright-report/`);
}
