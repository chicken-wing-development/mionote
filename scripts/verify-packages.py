"""Verify local release contents and hashes; this does not replace WACK/signing."""
import base64
import hashlib
import json
from pathlib import Path, PurePosixPath
import struct
import sys
import tomllib
import xml.etree.ElementTree as ET
import zipfile

ROOT = Path(__file__).resolve().parent.parent
DIST = ROOT / "dist"
MACHINES = {"arm64": 0xAA64, "x64": 0x8664}


def require(condition, message):
    if not condition:
        raise ValueError(message)


def digest(data):
    return hashlib.sha256(data).hexdigest()


def entries(archive):
    items = archive.infolist()
    require(len(items) <= 1024, "Too many archive entries")
    require(sum(i.file_size for i in items) <= 64 * 1024 * 1024, "Archive too large")
    names = [i.filename for i in items if not i.is_dir()]
    require(len(names) == len(set(names)), "Duplicate archive entries")
    for name in names:
        path = PurePosixPath(name)
        require(not path.is_absolute() and ".." not in path.parts and "\\" not in name and ":" not in name, "Unsafe archive path")
    require(archive.testzip() is None, "Archive CRC mismatch")
    return set(names)


def pe(data, architecture):
    require(data[:2] == b"MZ" and len(data) > 64, "Invalid executable")
    offset = struct.unpack_from("<I", data, 0x3C)[0]
    require(offset + 96 <= len(data) and data[offset:offset + 4] == b"PE\0\0", "Invalid PE header")
    require(struct.unpack_from("<H", data, offset + 4)[0] == MACHINES[architecture], "Wrong CPU")
    optional = offset + 24
    require(struct.unpack_from("<H", data, optional)[0] == 0x20B, "Expected 64-bit PE")
    require(struct.unpack_from("<H", data, optional + 68)[0] == 2, "Expected Windows GUI subsystem")
    flags = struct.unpack_from("<H", data, optional + 70)[0]
    require(flags & 0x4160 == 0x4160, "Missing ASLR/DEP/CFG flags")


def sha_file(path):
    require(Path(str(path) + ".sha256").read_text().split()[0] == digest(path.read_bytes()), "Release hash mismatch")


def shared_payload(store=False):
    result = {"LICENSE": ROOT / "LICENSE", "PRIVACY.txt": ROOT / "distribution/PRIVACY.txt",
              "THIRD-PARTY-NOTICES.txt": ROOT / "distribution/THIRD-PARTY-NOTICES.txt",
              "README.txt": ROOT / ("distribution/store/README.txt" if store else "distribution/README.txt")}
    for path in (ROOT / "distribution/licenses").rglob("*"):
        if path.is_file():
            result["licenses/" + path.relative_to(ROOT / "distribution/licenses").as_posix()] = path
    for name in ["security-review-20261027.md", "store-security-review-20261027.md", "dependency-audit-20261027.json"]:
        result["security/" + name] = ROOT / "docs" / name
    return result


def compare_payload(archive, prefix, payload):
    for name, path in payload.items():
        require(archive.read(prefix + name) == path.read_bytes(), "Payload differs: " + name)


def block_map(archive):
    blockmap = ET.fromstring(archive.read("AppxBlockMap.xml"))
    require(blockmap.attrib["HashMethod"].endswith("#sha256"), "Wrong block hash method")
    for file in blockmap:
        name = file.attrib["Name"].replace("\\", "/")
        data = archive.read(name)
        require(int(file.attrib["Size"]) == len(data), "Block map size mismatch")
        blocks = [item for item in file if item.tag == "{http://schemas.microsoft.com/appx/2010/blockmap}Block"]
        whole_hashes = [item for item in file if item.tag == "{http://schemas.microsoft.com/appx/2021/blockmap}FileHash"]
        require(len(blocks) + len(whole_hashes) == len(file), "Unknown block map element")
        require(len(blocks) == (len(data) + 65535) // 65536, "Block map count mismatch")
        for index, block in enumerate(blocks):
            expected = hashlib.sha256(data[index * 65536:(index + 1) * 65536]).digest()
            require(base64.b64decode(block.attrib["Hash"], validate=True) == expected, "MSIX block hash mismatch")
        for item in whole_hashes:
            require(base64.b64decode(item.attrib["Hash"], validate=True) == hashlib.sha256(data).digest(), "MSIX file hash mismatch")


def main():
    version = tomllib.loads((ROOT / "Cargo.toml").read_text())["package"]["version"]
    identity = json.loads((ROOT / "distribution/store/identity.json").read_text("utf-8"))
    audit = json.loads((ROOT / "docs/dependency-audit-20261027.json").read_text())
    require(audit["cargo_lock_sha256"] == digest((ROOT / "Cargo.lock").read_bytes()) and not audit["findings"], "Dependency audit is stale or has findings")
    for architecture in MACHINES:
        path = DIST / f"MioNote-{version}-portable-{architecture}.zip"
        sha_file(path)
        with zipfile.ZipFile(path) as archive:
            names = entries(archive)
            prefix = f"MioNote-{version}-portable-{architecture}/"
            payload = shared_payload()
            require(names == {prefix + n for n in payload} | {prefix + n for n in ["MioNote.exe", "SHA256SUMS.txt", "mio-note.portable"]}, "Unexpected portable payload (possible user data)")
            compare_payload(archive, prefix, payload)
            executable = archive.read(prefix + "MioNote.exe")
            pe(executable, architecture)
            require(archive.read(prefix + "mio-note.portable") == b"MioNote portable v1\n", "Portable marker mismatch")
            require(archive.read(prefix + "SHA256SUMS.txt").decode().split()[0] == digest(executable), "Executable hash mismatch")
        path = DIST / f"MioNote-{version}-store-{architecture}.msix"
        sha_file(path)
        with zipfile.ZipFile(path) as archive:
            names = entries(archive)
            payload = shared_payload(store=True)
            for name in ["StoreLogo.png", "Square44x44Logo.png", "Square150x150Logo.png"]:
                payload["Assets/" + name] = ROOT / "distribution/store/Assets" / name
            require(names == set(payload) | {"MioNote.exe", "AppxManifest.xml", "AppxBlockMap.xml", "[Content_Types].xml", "resources.pri"}, "Unexpected Store payload")
            compare_payload(archive, "", payload)
            pe(archive.read("MioNote.exe"), architecture)
            root = ET.fromstring(archive.read("AppxManifest.xml"))
            ns = {"p": "http://schemas.microsoft.com/appx/manifest/foundation/windows10"}
            item = root.find("p:Identity", ns)
            for field in ["Name", "Publisher"]:
                require(item.attrib[field] == identity[field], "Store identity mismatch")
            require(item.attrib["Version"] == identity["PackageVersion"] and item.attrib["ProcessorArchitecture"] == architecture, "Store version/CPU mismatch")
            require(root.find("p:Properties/p:PublisherDisplayName", ns).text == identity["PublisherDisplayName"], "Publisher display mismatch")
            capabilities = root.find("p:Capabilities", ns)
            require(len(capabilities) == 1 and capabilities[0].attrib["Name"] == "runFullTrust", "Unexpected capabilities")
            require("mio-note.portable" not in names, "Portable marker in Store build")
            block_map(archive)
        print(architecture + ": portable and Store payload, PE, identity, CRC and block hashes verified")
    bundle = DIST / f"MioNote-{version}-store.msixbundle"
    sha_file(bundle)
    with zipfile.ZipFile(bundle) as archive:
        entries(archive)
        block_map(archive)
        for architecture in MACHINES:
            name = f"MioNote-{version}-store-{architecture}.msix"
            require(archive.read(name) == (DIST / name).read_bytes(), "Bundle package mismatch")
    source = DIST / f"MioNote-{version}-source.zip"
    sha_file(source)
    with zipfile.ZipFile(source) as archive:
        names = entries(archive)
        prefix = f"MioNote-{version}-source/"
        manifest = prefix + "SOURCE-MANIFEST.sha256"
        expected = {manifest}
        for line in archive.read(manifest).decode().splitlines():
            hash_value, name = line.split("  ", 1)
            require(digest(archive.read(prefix + name)) == hash_value == digest((ROOT / name).read_bytes()), "Source differs: " + name)
            expected.add(prefix + name)
        require(names == expected, "Unexpected source file")
        require([n for n in names if n.endswith(".inknote")] == [prefix + "tests/fixtures/v2-mixed-paper.inknote"], "Unexpected notebook in source archive")
    print("Bundle and source archive verified; no user notes/settings/logs in release payloads")


if __name__ == "__main__":
    try:
        main()
    except (OSError, ValueError, KeyError, AttributeError, zipfile.BadZipFile, ET.ParseError) as error:
        print("Package verification failed: " + str(error), file=sys.stderr)
        sys.exit(1)
