import { describe, expect, it } from 'vitest';
import { PerspectiveCamera, Quaternion, Vector3 } from 'three';
import { isSceneCameraKey, moveSceneCamera, sceneCameraAxes, sceneCameraTurn, turnSceneCamera } from '../../src/editor/sceneCameraNavigation';

describe('scene camera navigation', () => {
  it('maps WASD and arrow key codes to opposing camera-relative axes', () => {
    expect(sceneCameraAxes(new Set())).toEqual({ forward: 0, right: 0, up: 0 });
    expect(sceneCameraAxes(new Set(['KeyW', 'ArrowUp']))).toEqual({ forward: 1, right: 0, up: 0 });
    expect(sceneCameraAxes(new Set(['KeyS', 'ArrowDown']))).toEqual({ forward: -1, right: 0, up: 0 });
    expect(sceneCameraAxes(new Set(['KeyD', 'ArrowRight']))).toEqual({ forward: 0, right: 1, up: 0 });
    expect(sceneCameraAxes(new Set(['KeyA', 'ArrowLeft']))).toEqual({ forward: 0, right: -1, up: 0 });
    expect(sceneCameraAxes(new Set(['KeyW', 'KeyS', 'KeyA', 'KeyD']))).toEqual({ forward: 0, right: 0, up: 0 });
  });

  it('maps Space/PageUp and C/PageDown to one world-vertical axis with cancellation', () => {
    expect(sceneCameraAxes(new Set(['Space']))).toEqual({ forward: 0, right: 0, up: 1 });
    expect(sceneCameraAxes(new Set(['PageUp']))).toEqual({ forward: 0, right: 0, up: 1 });
    expect(sceneCameraAxes(new Set(['Space', 'PageUp']))).toEqual({ forward: 0, right: 0, up: 1 });
    expect(sceneCameraAxes(new Set(['KeyC']))).toEqual({ forward: 0, right: 0, up: -1 });
    expect(sceneCameraAxes(new Set(['PageDown']))).toEqual({ forward: 0, right: 0, up: -1 });
    expect(sceneCameraAxes(new Set(['KeyC', 'PageDown']))).toEqual({ forward: 0, right: 0, up: -1 });
    expect(sceneCameraAxes(new Set(['Space', 'KeyC']))).toEqual({ forward: 0, right: 0, up: 0 });
    expect(sceneCameraAxes(new Set(['PageUp', 'PageDown']))).toEqual({ forward: 0, right: 0, up: 0 });
    for (const key of ['Space', 'KeyC', 'PageUp', 'PageDown']) expect(isSceneCameraKey(key)).toBe(true);
  });

  it('maps Q/E to opposite local yaw directions without stealing their outside-canvas shortcuts', () => {
    expect(sceneCameraTurn(new Set())).toBe(0);
    expect(sceneCameraTurn(new Set(['KeyQ']))).toBe(1);
    expect(sceneCameraTurn(new Set(['KeyE']))).toBe(-1);
    expect(sceneCameraTurn(new Set(['KeyQ', 'KeyE']))).toBe(0);
    expect(isSceneCameraKey('KeyQ')).toBe(true);
    expect(isSceneCameraKey('KeyE')).toBe(true);
    expect(isSceneCameraKey('KeyG')).toBe(false);
  });

  it('turns around local Y for a pitched and rolled camera while retaining position and pivot distance', () => {
    const camera = new PerspectiveCamera(45, 1, 0.1, 1000);
    camera.position.set(12, 7, 20);
    camera.rotation.set(0.31, -0.48, 0.22, 'XYZ');
    camera.updateMatrixWorld(true);
    const distance = 27.5;
    const target = camera.position.clone().add(camera.getWorldDirection(new Vector3()).multiplyScalar(distance));
    const beforePosition = camera.position.clone();
    const beforeQuaternion = camera.quaternion.clone();
    const angle = Math.PI / 7;

    turnSceneCamera(camera, target, angle);
    camera.updateMatrixWorld(true);

    const localTurn = beforeQuaternion.clone().invert().multiply(camera.quaternion);
    const expectedTurn = new Quaternion().setFromAxisAngle(new Vector3(0, 1, 0), angle);
    expect(localTurn.angleTo(expectedTurn)).toBeCloseTo(0, 10);
    expect(camera.position.distanceTo(beforePosition)).toBeCloseTo(0, 10);
    expect(camera.position.distanceTo(target)).toBeCloseTo(distance, 10);
    const forward = camera.getWorldDirection(new Vector3());
    const targetDirection = target.clone().sub(camera.position).normalize();
    expect(targetDirection.dot(forward)).toBeCloseTo(1, 10);
    expect(target.distanceTo(camera.position.clone().add(forward.multiplyScalar(distance)))).toBeCloseTo(0, 10);
  });

  it('rejects non-finite turn angles without moving either camera position or target', () => {
    const camera = new PerspectiveCamera(45, 1, 0.1, 1000);
    camera.position.set(1, 2, 3);
    const target = new Vector3(4, 5, 6);
    const beforePosition = camera.position.clone();
    const beforeTarget = target.clone();
    expect(() => turnSceneCamera(camera, target, Number.NaN)).toThrow(/finite/);
    expect(() => turnSceneCamera(camera, target, Number.POSITIVE_INFINITY)).toThrow(/finite/);
    expect(camera.position).toEqual(beforePosition);
    expect(target).toEqual(beforeTarget);
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

  it('moves vertically along world +Y for pitched and rolled cameras without changing orientation', () => {
    const camera = new PerspectiveCamera(45, 1, 0.1, 1000);
    camera.position.set(12, 7, 20);
    camera.rotation.set(0.41, -0.63, 0.28, 'XYZ');
    camera.updateMatrixWorld(true);
    const target = camera.position.clone().add(camera.getWorldDirection(new Vector3()).multiplyScalar(19));
    const beforePosition = camera.position.clone();
    const beforeTarget = target.clone();
    const beforeQuaternion = camera.quaternion.clone();
    const beforeOffset = beforePosition.clone().sub(beforeTarget);

    moveSceneCamera(camera, target, 0, 0, 4, 1);

    const delta = camera.position.clone().sub(beforePosition);
    expect(delta.x).toBeCloseTo(0, 10);
    expect(delta.y).toBeCloseTo(4, 10);
    expect(delta.z).toBeCloseTo(0, 10);
    expect(target.clone().sub(beforeTarget).distanceTo(delta)).toBeCloseTo(0, 10);
    expect(camera.quaternion.angleTo(beforeQuaternion)).toBeCloseTo(0, 6);
    expect(camera.position.clone().sub(target).distanceTo(beforeOffset)).toBeCloseTo(0, 10);
  });

  it('normalizes mixed camera-relative and world-vertical movement as one vector', () => {
    const camera = new PerspectiveCamera(45, 1, 0.1, 1000);
    camera.position.set(8, 9, 13);
    camera.rotation.set(-0.27, 0.54, 0.36, 'XYZ');
    camera.updateMatrixWorld(true);
    const target = camera.position.clone().add(camera.getWorldDirection(new Vector3()).multiplyScalar(23));
    const beforePosition = camera.position.clone();
    const beforeTarget = target.clone();
    const beforeQuaternion = camera.quaternion.clone();
    const forward = camera.getWorldDirection(new Vector3());
    const right = new Vector3().setFromMatrixColumn(camera.matrixWorld, 0).normalize();
    const expected = forward.clone().addScaledVector(right, -1).add(new Vector3(0, 1, 0)).normalize().multiplyScalar(6);

    moveSceneCamera(camera, target, 1, -1, 6, 1);

    const actual = camera.position.clone().sub(beforePosition);
    expect(actual.distanceTo(expected)).toBeCloseTo(0, 10);
    expect(target.clone().sub(beforeTarget).distanceTo(actual)).toBeCloseTo(0, 10);
    expect(actual.length()).toBeCloseTo(6, 10);
    expect(camera.quaternion.angleTo(beforeQuaternion)).toBeCloseTo(0, 10);
  });

  it('rejects non-finite vertical input without moving the camera or target', () => {
    const camera = new PerspectiveCamera(45, 1, 0.1, 1000);
    camera.position.set(1, 2, 3);
    const target = new Vector3(4, 5, 6);
    const beforePosition = camera.position.clone();
    const beforeTarget = target.clone();
    expect(() => moveSceneCamera(camera, target, 0, 0, 1, Number.NaN)).toThrow(/finite/);
    expect(() => moveSceneCamera(camera, target, 0, 0, 1, Number.POSITIVE_INFINITY)).toThrow(/finite/);
    expect(camera.position).toEqual(beforePosition);
    expect(target).toEqual(beforeTarget);
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
