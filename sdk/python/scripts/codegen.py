"""Generate gRPC stubs from the repo-level protos.

The protos directory is the single source of truth; generated code is never
checked in. Run after syncing dev dependencies:

    uv run python scripts/codegen.py

Protos are located via `PROTOS_DIR`, else `<repo>/protos` next to a checkout,
else `/protos` (the agent Dockerfile layout).
"""

from __future__ import annotations

import os
import pathlib
import re
import subprocess
import sys

ROOT = pathlib.Path(__file__).resolve().parents[1]
OUT_DIR = ROOT / "src" / "achtung" / "_generated"

_GRPC_IMPORT = re.compile(r"^import (\w+_pb2) as (\w+)$", re.MULTILINE)


def _find_protos() -> pathlib.Path:
    if override := os.environ.get("PROTOS_DIR"):
        protos = pathlib.Path(override)
    elif (ROOT.parent.parent / "protos" / "achtung_agent.proto").exists():
        protos = ROOT.parent.parent / "protos"
    else:
        protos = pathlib.Path("/protos")
    if not (protos / "achtung_agent.proto").exists():
        raise RuntimeError(f"achtung_agent.proto not found under {protos}")
    return protos


def main() -> None:
    protos_dir = _find_protos()
    OUT_DIR.mkdir(parents=True, exist_ok=True)
    (OUT_DIR / "__init__.py").touch(exist_ok=True)
    subprocess.run(
        [
            sys.executable,
            "-m",
            "grpc_tools.protoc",
            f"-I{protos_dir}",
            f"--python_out={OUT_DIR}",
            f"--grpc_python_out={OUT_DIR}",
            f"--mypy_out={OUT_DIR}",
            "achtung_agent.proto",
        ],
        check=True,
    )
    # grpc_tools emits bare `import xxx_pb2` in the service stub, which only
    # resolves on sys.path. Rewrite to a package-relative import instead.
    for stub in OUT_DIR.glob("*_pb2_grpc.py"):
        text = stub.read_text()
        text, count = _GRPC_IMPORT.subn(r"from achtung._generated import \1 as \2", text)
        if count == 0:
            raise RuntimeError(f"expected bare pb2 import in {stub.name}")
        stub.write_text(text)
    print(f"generated stubs in {OUT_DIR}")


if __name__ == "__main__":
    main()
