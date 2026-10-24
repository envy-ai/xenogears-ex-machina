/** Read-only Scene observations; the existing controller owns paused commands. */
import { useEffect, useRef, useState } from 'react';
import { useLiveGame } from './LiveGame';
import { readScenePose } from './scene-pose';
import { useSceneRuntime } from '../events/EventStudio';

export function SceneRuntimePose({ report }: { report(error: unknown): void }) {
  const { controller, state } = useLiveGame();
  const scene = useSceneRuntime();
  const [reading, setReading] = useState(false);
  const [error, setError] = useState<Error | null>(null);
  const attempt = useRef(0);
  useEffect(() => {
    ++attempt.current;
    scene.publish(null);
    setError(null);
    return () => { ++attempt.current; };
  }, [controller, state.schema, state.revision, scene.folder, scene.session, scene.revision, scene.source, scene.selected, scene.publish]);
  const refresh = async () => {
    const identity = controller.getSnapshot().schema?.identity;
    if (!identity) throw new Error('Attach a matching runtime before reading its actor pose');
    const observation = scene.capture(identity), current = ++attempt.current;
    setReading(true); setError(null);
    try {
      const pose = await controller.paused(request => readScenePose(request, observation.source));
      observation.assertCurrent();
      if (attempt.current !== current) throw new Error('The runtime/Scene observation owner changed; no pose was adopted');
      scene.publish(pose);
    } finally { setReading(false); }
  };
  const run = (operation: () => Promise<void>) => {
    scene.publish(null);
    void operation().catch(cause => {
      const error = cause instanceof Error ? cause : new Error(String(cause));
      setError(error); report(error);
    });
  };
  const blockedReason = !scene.active ? 'Open the Scene view before observing its runtime source.'
    : !state.connected ? 'Attach a matching runtime first.'
    : state.busy ? 'Wait for the current runtime operation.'
    : reading ? 'Wait for the current pose observation.'
    : scene.pending ? 'Wait for the current project or map operation.'
    : scene.inlineDirty ? 'Apply or Cancel the inline fields first.' : null;
  const bindReason = blockedReason ?? (state.schema?.identity.packages.length ? 'Source context requires a vanilla runtime with no enabled packages.' : null);
  const refreshReason = blockedReason ?? (!scene.actor ? 'Select an actor with a qualified runtime identity.' : null);
  return <div className="scene-pose">
    {scene.pose ? <>
      <p className="editor-text editor-text--caption">Observed VBlank {scene.pose.vblank} · Actor {scene.pose.actor} · {scene.pose.entity}</p>
      <dl className="scene-pose-values">{scene.pose.position.map((value, axis) => <div key={axis}><dt className="editor-text editor-text--caption">{['X', 'Y', 'Z'][axis]} · observed</dt><dd className="editor-text editor-text--code">{value}</dd></div>)}</dl>
      <p className="editor-text editor-text--caption">Signed heading word: {scene.pose.heading_raw} · orange marker is read-only</p>
    </> : <p className="editor-text editor-text--caption">{!state.connected ? 'Disconnected. Attach the matching runtime to observe a pose.' : !scene.actor ? 'Select an actor with a qualified runtime identity.' : 'No pose captured for this source/actor revision.'}</p>}
    <details className="scene-pose-controls"><summary className="editor-text">Runtime observation controls</summary><div className="editor-field-list">
      <p className="editor-text editor-text--caption">Both actions pause the runtime and leave it paused; resume from Live game. Live values never write authored placement.</p>
      <button className="editor-button" disabled={Boolean(bindReason)} title={bindReason??undefined} onClick={() => run(scene.bindContext)}>Bind original runtime source context</button>
      <p className="editor-text editor-text--caption">Binding requires a vanilla runtime and records measured provenance as unsaved project data.</p>
      <button className="editor-button" disabled={Boolean(refreshReason)} title={refreshReason??undefined} onClick={() => run(refresh)}>Refresh observed actor pose</button>
    </div></details>
    {error && <details role="alert"><summary className="editor-text editor-text--danger">{error.message}</summary><pre className="editor-text editor-text--code">{error.stack ?? error.message}</pre></details>}
  </div>;
}
