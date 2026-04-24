#!/usr/bin/env python3

from typing import Any, cast
from pathlib import Path
from tomlkit.items import Table
from tomlkit.toml_document import TOMLDocument
import subprocess
import sys
import tomlkit

def run(cmd):
    subprocess.run(cmd, check=True)

def update_workspace_dependency(crate_name: str):
    root = Path("Cargo.toml")
    doc = tomlkit.parse(root.read_text())

    if "workspace" not in doc:
        doc["workspace"] = tomlkit.table()

    workspace = doc["workspace"]

    if not isinstance(workspace, Table):
        raise TypeError("[workspace] is not a TOML table")

    if "dependencies" not in workspace:
        workspace["dependencies"] = tomlkit.table()

    deps = workspace["dependencies"]

    if not isinstance(deps, Table):
        raise TypeError("[workspace.dependencies] is not a TOML table")

    if crate_name in deps:
        print(f"ℹ️  {crate_name} already exists")
        return

    inline = tomlkit.inline_table()
    inline["path"] = f"crates/{crate_name}"

    deps[crate_name] = inline

    root.write_text(tomlkit.dumps(doc))

    run(["tombi", "format", "Cargo.toml"])

def main():
    if len(sys.argv) < 2:
        print("usage: makers <name>")
        sys.exit(1)

    name = sys.argv[1]
    crate_dir = Path("crates") / name

    # ① cargo new
    run(["cargo", "new", "--lib", str(crate_dir), "--vcs", "none"])

    # ② lib.rs → foo.rs
    src_dir = crate_dir / "src"
    lib_rs = src_dir / "lib.rs"
    new_rs = src_dir / f"{name}.rs"
    lib_rs.rename(new_rs)

    # ③ Cargo.toml 編集
    cargo_toml_path = crate_dir / "Cargo.toml"
    doc = cast(Any, tomlkit.parse(cargo_toml_path.read_text()))

    if "lib" not in doc:
        doc["lib"] = tomlkit.table()

    doc["lib"]["name"] = name
    doc["lib"]["path"] = f"src/{name}.rs"

    cargo_toml_path.write_text(tomlkit.dumps(doc))

    # ④ workspace.dependencies に追加
    update_workspace_dependency(name)

    # format
    run(["tombi", "format", cargo_toml_path])

    print(f"🎉 created crate: {name}")


if __name__ == "__main__":
    main()
