# Desktop runtime notices

Codewhale-MedSci preserves the repository's MIT license and upstream attribution.
The desktop shell is built with Tauri, Leptos and Rust dependencies locked in
`apps/desktop/Cargo.lock`; the Agent dependencies are locked in the root Cargo.lock.

Python is the Astral python-build-standalone CPython 3.12.13 distribution. Its
original LICENSE and distribution files remain in the application payload.
Python package wheels retain their original `.dist-info` license/NOTICE files;
`runtime/wheelhouse-manifest.json` records package versions, SHA-256, declared
license metadata and PyPI source links. `runtime/ocr-models-manifest.json` records
the actual model byte hashes. Platform-specific manifests must ship alongside
that platform's wheelhouse.

PyMuPDF declares AGPL licensing. Its inclusion needs an explicit distribution
license review before any public or enterprise release. This development build
is not a completed legal/license clearance. OCR model licenses and conversion
provenance also need their own release review; Python package metadata alone
is not a model-license determination.

No CUDA, PyTorch, Paddle training distribution or onnxruntime-gpu is included.

Windows preview.7.3 includes Microsoft Visual C++ x64 redistributable CRT libraries deployed app-locally. Source: https://aka.ms/vs/17/release/vc_redist.x64.exe . The exact source and individual DLL SHA-256 hashes are retained in resources/python/vc-runtime/manifest.json. These libraries remain Microsoft components.

Windows preview.7.4 bundles the complete official PortableGit 2.55.0.windows.5 distribution, including Bash/MSYS and Git. Original LICENSE.txt and component licenses remain in resources/git-bash. The official release archive URL, SHA-256 and every packaged file hash are recorded in medsci-git-manifest.json. Source/release information: https://github.com/git-for-windows/git/releases/tag/v2.55.0.windows.5 .
