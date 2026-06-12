#!/usr/bin/env python3

import argparse
import json
import os
import shutil
import subprocess
import sys
import tempfile
from pathlib import Path


def parse_arguments() -> argparse.Namespace:
    parser = argparse.ArgumentParser(
        description="Generate third-party license notices from Cargo.lock.",
    )
    parser.add_argument(
        "--output",
        default="THIRD_PARTY_NOTICES.md",
        help="Markdown notice file to write.",
    )
    parser.add_argument(
        "--json-output",
        default="THIRD_PARTY_LICENSES.json",
        help="Raw cargo-bundle-licenses JSON file to write.",
    )
    return parser.parse_args()


def run_bundle_licenses(json_path: Path) -> None:
    executable = shutil.which("cargo-bundle-licenses")
    if executable is None:
        cargo_home_executable = Path.home() / ".cargo" / "bin" / "cargo-bundle-licenses"
        if cargo_home_executable.exists():
            executable = str(cargo_home_executable)

    if executable is None:
        print(
            "cargo-bundle-licenses is required. Install it with:\n"
            "  cargo install cargo-bundle-licenses --locked",
            file=sys.stderr,
        )
        sys.exit(1)

    environment = os.environ.copy()
    environment.setdefault("RUST_LOG", "error")

    subprocess.run(
        [
            executable,
            "--format",
            "json",
            "--output",
            str(json_path),
            "--prefer",
            "MIT",
            "--prefer",
            "Apache-2.0",
        ],
        check=True,
        env=environment,
    )


def load_cargo_packages() -> dict[tuple[str, str], dict]:
    metadata_output = subprocess.check_output(
        ["cargo", "metadata", "--locked", "--format-version", "1"],
        text=True,
    )
    metadata = json.loads(metadata_output)
    return {
        (package["name"], package["version"]): package
        for package in metadata["packages"]
        if package["id"] not in set(metadata["workspace_members"])
    }


def fill_missing_license_files(bundle: dict, cargo_packages: dict[tuple[str, str], dict]) -> None:
    for library in bundle["third_party_libraries"]:
        licenses = library.get("licenses") or []
        has_missing_license = (
            library.get("license") == "No license specified"
            or any(license_entry.get("text") == "NOT FOUND" for license_entry in licenses)
        )
        if not has_missing_license:
            continue

        cargo_package = cargo_packages.get(
            (library["package_name"], library["package_version"]),
        )
        if cargo_package is None:
            continue

        manifest_path = Path(cargo_package["manifest_path"])
        license_entries = read_license_entries(manifest_path.parent)
        if not license_entries:
            continue

        library["license"] = " OR ".join(
            license_entry["license"] for license_entry in license_entries
        )
        library["licenses"] = license_entries
        if cargo_package.get("repository") and not library.get("repository"):
            library["repository"] = cargo_package["repository"]


def fill_missing_license_references(bundle: dict) -> None:
    for library in bundle["third_party_libraries"]:
        for license_entry in library.get("licenses", []):
            if license_entry.get("text") != "NOT FOUND":
                continue

            license_name = license_entry["license"]
            license_entry["text"] = (
                "The package declares this SPDX license identifier, but no "
                "license file was found in the crate source.\n\n"
                "SPDX license reference: {license_url}"
            ).format(
                license_url=spdx_license_url(license_name),
            )


def spdx_license_url(license_name: str) -> str:
    escaped_license_name = license_name.replace(" ", "%20")
    return "https://spdx.org/licenses/{license_name}.html".format(
        license_name=escaped_license_name,
    )


def read_license_entries(package_directory: Path) -> list[dict[str, str]]:
    license_entries = []
    for path in sorted(package_directory.iterdir()):
        if not path.is_file() or not is_notice_file(path.name):
            continue

        license_entries.append(
            {
                "license": infer_license_name(path.name),
                "text": path.read_text(errors="replace"),
            }
        )

    return license_entries


def is_notice_file(file_name: str) -> bool:
    normalized_name = file_name.upper()
    return (
        normalized_name.startswith("LICENSE")
        or normalized_name.startswith("LICENCE")
        or normalized_name.startswith("COPYING")
        or normalized_name.startswith("NOTICE")
    )


def infer_license_name(file_name: str) -> str:
    normalized_name = file_name.upper()
    if "APACHE" in normalized_name:
        return "Apache-2.0"
    if "MIT" in normalized_name:
        return "MIT"
    if "BSD" in normalized_name:
        return "BSD"
    if "GPL" in normalized_name:
        return "GPL"
    if "NOTICE" in normalized_name:
        return "NOTICE"
    return file_name


def render_markdown(bundle: dict) -> str:
    libraries = sorted(
        bundle["third_party_libraries"],
        key=lambda library: (
            library["package_name"].lower(),
            library["package_version"],
        ),
    )

    lines = [
        "# Third-Party Notices",
        "",
        "This product includes third-party open source components.",
        "",
        "The application source itself is licensed under GPL-3.0-only. See `LICENSE`.",
        "Third-party components remain subject to their own license terms.",
        "",
        "This file is generated from Cargo.lock with `scripts/generate-third-party-notices.py`.",
        "Review the generated notices before distributing a release build.",
        "",
        "## Packages",
        "",
        "| Package | Version | License | Repository |",
        "| --- | --- | --- | --- |",
    ]

    for library in libraries:
        repository = library.get("repository") or ""
        lines.append(
            "| {package_name} | {package_version} | {license} | {repository} |".format(
                package_name=escape_table_cell(library["package_name"]),
                package_version=escape_table_cell(library["package_version"]),
                license=escape_table_cell(library.get("license") or "UNKNOWN"),
                repository=escape_table_cell(repository),
            )
        )

    lines.extend(["", "## License Texts", ""])

    for library in libraries:
        lines.extend(
            [
                "### {package_name} {package_version}".format(
                    package_name=library["package_name"],
                    package_version=library["package_version"],
                ),
                "",
                "- License: `{license}`".format(
                    license=library.get("license") or "UNKNOWN",
                ),
            ]
        )

        repository = library.get("repository")
        if repository:
            lines.append("- Repository: {repository}".format(repository=repository))

        for license_entry in library["licenses"]:
            lines.extend(
                [
                    "",
                    "#### {license_name}".format(
                        license_name=license_entry["license"],
                    ),
                    "",
                    "```text",
                    license_entry["text"].rstrip(),
                    "```",
                ]
            )

        lines.append("")

    return "\n".join(lines)


def escape_table_cell(value: str) -> str:
    return value.replace("\\", "\\\\").replace("|", "\\|").replace("\n", " ")


def main() -> None:
    arguments = parse_arguments()
    output_path = Path(arguments.output)
    json_output_path = Path(arguments.json_output)

    with tempfile.TemporaryDirectory() as temporary_directory:
        temporary_json_path = Path(temporary_directory) / "third-party-licenses.json"
        run_bundle_licenses(temporary_json_path)
        bundle = json.loads(temporary_json_path.read_text())

    fill_missing_license_files(bundle, load_cargo_packages())
    fill_missing_license_references(bundle)

    json_output_path.write_text(json.dumps(bundle, indent=2, ensure_ascii=False) + "\n")
    output_path.write_text(render_markdown(bundle))


if __name__ == "__main__":
    main()
