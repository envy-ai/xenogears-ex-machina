# Studio dependency security updates (2026-10-20)

The baseline `npm audit --json` reported three affected packages (`vite`,
`vitest`, `@vitest/mocker`) representing four distinct advisories. The package
summary was one high and two moderate; the Vite findings themselves were two
moderate and one high. This inventory does not establish exploitability on
this host.

| Primary advisory | Conditions and this studio's configuration |
| --- | --- |
| [GHSA-r825-w8mm-5pxw](https://github.com/advisories/GHSA-r825-w8mm-5pxw) | For Vite, exploitation requires WSL, an attacker-created filesystem entry and a listed editor such as Vim or Emacs. Shodan runs native Linux, so that Vite-specific condition is absent. This is not evidence that any LAN attacker can execute commands here. |
| [GHSA-698v-4r3x-3vcv](https://github.com/advisories/GHSA-698v-4r3x-3vcv) | Windows command escaping: a malicious filename and environment-variable injection or NTFS stream conditions can reach shell commands. It does not apply to this Linux host. |
| [GHSA-587h-7wmj-h5pm](https://github.com/advisories/GHSA-587h-7wmj-h5pm) | Windows-only existence-check bypass. Together with the escaping issue, a crafted editor file argument can permit command injection without placing the file. Its Windows prerequisite is absent here. |
| [GHSA-82fw-gwwq-j7x9](https://github.com/advisories/GHSA-82fw-gwwq-j7x9) | Arbitrary file reading through redirect mocks is exposed by the public `mockerPlugin`/`interceptorPlugin` configurations. Studio's Vite config registers the React plugin, not those mock plugins; Vitest runs Node unit tests. The described unauthenticated dev-server configuration is absent. This assessment must be reconsidered if plugins or test modes change. |

All three editor advisories list Vite 8.1.6 as a patched version. The mocker
advisory lists Vitest/mocker 4.1.11 as patched. We selected exact Vite **8.1.6**
and Vitest **4.1.12**, rather than upgrading Vite's minor line. The required
Vitest family packages also move from 4.1.10 to 4.1.12. No other package versions
changed in the lockfile.

Before updating, metadata was checked against the official npm registry:
[Vite 8.1.6](https://registry.npmjs.org/vite/8.1.6),
[Vitest 4.1.12](https://registry.npmjs.org/vitest/4.1.12), and
[React plugin 6.0.3](https://registry.npmjs.org/@vitejs%2fplugin-react/6.0.3).
Vite accepts Node `^20.19.0 || >=22.12.0`; Vitest accepts Node 24 and Vite 8;
the existing React plugin accepts Vite 8. The pinned Node 24.19.0 and existing
Sass/React/plugin pins satisfy those requirements. Other direct pins, including
NW.js, remain unchanged.

Both the scoped install and the separate final pinned audit reported zero
vulnerabilities. Audit results are a timestamped registry inventory,
not proof that every dependency or future configuration is safe. Installation
also reported unapproved lifecycle scripts for the existing NW.js and optional
watcher packages; their permissions were not changed, and verification uses
the previously installed, checksum-recorded NW.js binary.
