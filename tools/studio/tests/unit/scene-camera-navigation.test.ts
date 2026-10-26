import { describe, expect, it } from 'vitest';
import { PerspectiveCamera, Vector3 } from 'three';
import { moveSceneCamera, sceneCameraAxes } from '../../src/editor/sceneCameraNavigation';

describe('scene camera navigation', () => {
  it('maps WASD and arrow key codes to opposing camera-relative axes', () => {
    expect(sceneCameraAxes(new Set())).toEqual({ forward: 0, right: 0 });
    expect(sceneCameraAxes(new Set(['KeyW', 'ArrowUp']))).toEqual({ forward: 1, right: 0 });
    expect(sceneCameraAxes(new Set(['KeyS', 'ArrowDown']))).toEqual({ forward: -1, right: 0 });
    expect(sceneCameraAxes(new Set(['KeyD', 'ArrowRight']))).toEqual({ forward: 0, right: 1 });
    expect(sceneCameraAxes(new Set(['KeyA', 'ArrowLeft']))).toEqual({ forward: 0, right: -1 });
    expect(sceneCameraAxes(new Set(['KeyW', 'KeyS', 'KeyA', 'KeyD']))).toEqual({ forward: 0, right: 0 });
  });

  it('moves camera and orbit target by the same exact cardinal distance', () => {
    const camera = new PerspectiveCamera(45, 1, 0.1, 1000);
    camera.position.set(12, 7, 20);
    const target = new Vector3(2, 1, 3);
    camera.lookAt(target);
    camera.updateMatrixWorld(true);
    const beforePosition = camera.position.clone();
    const beforeTarget = target.clone();
    const beforeOffset = beforePosition.clone().sub(beforeTarget);

    moveSceneCamera(camera, target, 1, 0, 5);

    const positionDelta = camera.position.clone().sub(beforePosition);
    const targetDelta = target.clone().sub(beforeTarget);
    expect(positionDelta.length()).toBeCloseTo(5, 10);
    expect(targetDelta.distanceTo(positionDelta)).toBeCloseTo(0, 10);
    expect(camera.position.clone().sub(target).distanceTo(beforeOffset)).toBeCloseTo(0, 10);
  });

  it('normalizes diagonal input to the same distance and can dolly past a near target', () => {
    const camera = new PerspectiveCamera(45, 1, 0.1, 1000);
    camera.position.set(0, 0, 0);
    const target = new Vector3(0, 0, -0.1);
    camera.lookAt(target);
    camera.updateMatrixWorld(true);
    const beforePosition = camera.position.clone();
    const beforeTarget = target.clone();

    moveSceneCamera(camera, target, 1, 1, 3);
    const diagonalDelta = camera.position.clone().sub(beforePosition);
    expect(diagonalDelta.length()).toBeCloseTo(3, 10);
    expect(target.clone().sub(beforeTarget).distanceTo(diagonalDelta)).toBeCloseTo(0, 10);

    const nearCamera = new PerspectiveCamera(45, 1, 0.1, 1000);
    nearCamera.position.set(0, 0, 0);
    const nearTarget = new Vector3(0, 0, -0.1);
    nearCamera.lookAt(nearTarget);
    nearCamera.updateMatrixWorld(true);
    moveSceneCamera(nearCamera, nearTarget, 1, 0, 1);
    expect(nearCamera.position.z).toBeLessThan(-0.9);
    expect(nearTarget.z).toBeLessThan(-1);
    expect(nearCamera.position.distanceTo(nearTarget)).toBeCloseTo(0.1, 10);
  });
});
