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
    void operation().catch(cause => {
      const error = cause instanceof Error ? cause : new Error(String(cause));
      setError(error); report(error);
    });
  };
  const blocked = !scene.active || !state.connected || Boolean(state.busy) || reading || scene.pending || scene.inlineDirty;
  return <div className="editor-field-list">
    <p className="editor-text editor-text--caption">Both actions pause the runtime and leave it paused; resume from Live game. Live values never write authored placement.</p>
    <button className="editor-button" disabled={blocked || Boolean(state.schema?.identity.packages.length)} onClick={() => run(scene.bindContext)}>Bind original runtime source context</button>
    <p className="editor-text editor-text--caption">Binding requires a vanilla runtime and records measured provenance as unsaved project data.</p>
    <button className="editor-button" disabled={blocked || !scene.actor} onClick={() => run(refresh)}>Refresh observed actor pose</button>
    {scene.pose ? <>
      <p className="editor-text editor-text--caption">Observed VBlank {scene.pose.vblank} · Actor {scene.pose.actor} · {scene.pose.entity}</p>
      <dl className="scene-pose-values">{scene.pose.position.map((value, axis) => <div key={axis}><dt className="editor-text editor-text--caption">{['X', 'Y', 'Z'][axis]} · source units</dt><dd className="editor-text editor-text--code">{value}</dd></div>)}</dl>
      <p className="editor-text editor-text--caption">Signed heading word: {scene.pose.heading_raw} · orange marker is read-only</p>
    </> : <p className="editor-text editor-text--caption">{!state.connected ? 'Disconnected. Attach the matching runtime to observe a pose.' : !scene.actor ? 'Select an actor with a qualified runtime identity.' : 'No pose captured for this source/actor revision.'}</p>}
    {error && <details role="alert"><summary className="editor-text editor-text--danger">{error.message}</summary><pre className="editor-text editor-text--code">{error.stack ?? error.message}</pre></details>}
  </div>;
}
