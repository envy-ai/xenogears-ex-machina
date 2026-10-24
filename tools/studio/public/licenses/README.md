# Rendering dependency notices

`three/LICENSE` is the unmodified MIT copyright and permission notice from
`three@0.186.1`, pinned by `tools/studio/package-lock.json` to
`https://registry.npmjs.org/three/-/three-0.186.1.tgz` (package integrity is in that
lockfile). Its SHA-256 is `8b378ebe60e2fe500158cb0ac71cb5e8b7d92953c2abcc63a0eb90499653b5bc`. The notice is copied from the installed
pinned package without adding a source header inside the licence text.

Vite copies this public directory into the built Studio distribution. This
notice covers the Three.js renderer and its imported controls; the scene/domain
code remains newly authored. No game assets or runtime source context belong in
this directory. Font notices remain with their font files under `public/fonts`.
