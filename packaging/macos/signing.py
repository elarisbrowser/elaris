#!/usr/bin/env python3
"""Prepare, sign, notarize, and verify an Elaris macOS release bundle."""

from __future__ import annotations

import argparse
import base64
import hashlib
import json
import os
import plistlib
import re
import shlex
import shutil
import signal
import stat
import subprocess
import sys
import tempfile
import time
import uuid
from dataclasses import dataclass
from pathlib import Path
from typing import Mapping, Sequence
from xml.parsers.expat import ExpatError

ROOT = Path(__file__).resolve().parents[2]
CODE_BUNDLE_SUFFIXES = {".app", ".appex", ".bundle", ".framework", ".plugin", ".xpc"}
ENTITLEMENTS_ALLOWED = {"com.apple.security.cs.allow-jit"}
REQUIRED_CREDENTIALS = (
    "APPLE_CERTIFICATE",
    "APPLE_CERTIFICATE_PASSWORD",
    "APPLE_TEAM_ID",
    "APPLE_ID",
    "APPLE_APP_PASSWORD",
)


class SigningError(RuntimeError):
    """A release signing or verification step failed."""


@dataclass(frozen=True)
class Credentials:
    certificate: bytes
    certificate_password: str
    team_id: str
    apple_id: str
    app_password: str

    def secret_values(self) -> tuple[str, ...]:
        return (self.certificate_password, self.apple_id, self.app_password)


@dataclass(frozen=True)
class SigningTarget:
    path: Path
    entitlements: Path | None = None
    is_bundle: bool = False


def _run(
    command: Sequence[str | Path],
    *,
    check: bool = True,
    quiet_failure: bool = False,
    timeout: int | None = None,
) -> subprocess.CompletedProcess[str]:
    args = [str(part) for part in command]
    child_environment = os.environ.copy()
    for name in REQUIRED_CREDENTIALS:
        child_environment.pop(name, None)
    try:
        result = subprocess.run(
            args, capture_output=True, text=True, check=False, timeout=timeout, env=child_environment,
        )
    except subprocess.TimeoutExpired as error:
        limit = f" after {timeout} seconds" if timeout is not None else ""
        raise SigningError(f"{Path(args[0]).name} timed out{limit}.") from error
    if check and result.returncode:
        detail = "" if quiet_failure else (result.stderr or result.stdout).strip()
        message = f"{Path(args[0]).name} failed with exit status {result.returncode}"
        if detail:
            message += f": {detail}"
        raise SigningError(message)
    return result


def _safe_output(text: str, credentials: Credentials) -> str:
    for secret in credentials.secret_values():
        if secret:
            text = text.replace(secret, "[redacted]")
    return text.strip()


def load_credentials(environment: Mapping[str, str] | None = None) -> Credentials:
    """Read and validate all production credentials without logging their values."""
    values = os.environ if environment is None else environment
    missing = [name for name in REQUIRED_CREDENTIALS if not values.get(name)]
    if missing:
        raise SigningError(
            "Missing required Apple signing credentials: " + ", ".join(missing)
            + ". Configure them in the protected apple-signing GitHub environment."
        )
    team_id = values["APPLE_TEAM_ID"].strip()
    if not re.fullmatch(r"[A-Z0-9]{10}", team_id):
        raise SigningError("APPLE_TEAM_ID must be the 10-character Apple Developer team identifier.")
    try:
        encoded_certificate = re.sub(r"\s+", "", values["APPLE_CERTIFICATE"])
        certificate = base64.b64decode(encoded_certificate, validate=True)
    except (ValueError, base64.binascii.Error) as error:
        raise SigningError("APPLE_CERTIFICATE must contain a Base64-encoded .p12 certificate.") from error
    if not certificate:
        raise SigningError("APPLE_CERTIFICATE decodes to an empty file.")
    return Credentials(
        certificate=certificate,
        certificate_password=values["APPLE_CERTIFICATE_PASSWORD"],
        team_id=team_id,
        apple_id=values["APPLE_ID"],
        app_password=values["APPLE_APP_PASSWORD"],
    )


class TemporaryKeychain:
    """Import one signing identity into an isolated keychain and always remove it."""

    def __init__(self, credentials: Credentials):
        self.credentials = credentials
        self.directory: Path | None = None
        self.path: Path | None = None
        self.password = uuid.uuid4().hex + uuid.uuid4().hex
        self.original_search_list: list[str] = []
        self.search_list_changed = False
        self.created = False

    def __enter__(self) -> Path:
        self.directory = Path(tempfile.mkdtemp(prefix="elaris-signing-"))
        self.directory.chmod(stat.S_IRWXU)
        self.path = self.directory / "signing.keychain-db"
        certificate_path = self.directory / "developer-id.p12"
        certificate_path.write_bytes(self.credentials.certificate)
        certificate_path.chmod(stat.S_IRUSR | stat.S_IWUSR)
        try:
            _run(["security", "create-keychain", "-p", self.password, self.path], quiet_failure=True)
            self.created = True
            _run(["security", "set-keychain-settings", "-lut", "3600", self.path], quiet_failure=True)
            _run(["security", "unlock-keychain", "-p", self.password, self.path], quiet_failure=True)
            _run(
                [
                    "security", "import", certificate_path, "-k", self.path, "-P",
                    self.credentials.certificate_password, "-T", "/usr/bin/codesign",
                ],
                quiet_failure=True,
            )
            _run(
                [
                    "security", "set-key-partition-list", "-S", "apple-tool:,apple:", "-s",
                    "-k", self.password, self.path,
                ],
                quiet_failure=True,
            )
            listing = _run(["security", "list-keychains", "-d", "user"])
            self.original_search_list = shlex.split(listing.stdout)
            self.search_list_changed = True
            _run(["security", "list-keychains", "-d", "user", "-s", self.path, *self.original_search_list])
            certificate_path.unlink(missing_ok=True)
            return self.path
        except BaseException:
            self.__exit__(*sys.exc_info())
            raise

    def __exit__(self, exc_type, exc_value, traceback) -> bool:
        failures: list[str] = []
        if self.search_list_changed:
            try:
                _run(["security", "list-keychains", "-d", "user", "-s", *self.original_search_list])
            except (OSError, SigningError) as error:
                failures.append(f"could not restore the user's keychain search list ({error})")
        if self.created and self.path is not None:
            try:
                _run(["security", "lock-keychain", self.path], quiet_failure=True)
            except (OSError, SigningError) as error:
                failures.append(f"could not lock the temporary keychain ({error})")
            try:
                _run(["security", "delete-keychain", self.path], quiet_failure=True)
            except (OSError, SigningError) as error:
                failures.append(f"could not delete the temporary keychain ({error})")
        if self.directory is not None:
            try:
                shutil.rmtree(self.directory)
            except OSError as error:
                failures.append(f"could not remove temporary signing files ({error})")
        if failures:
            message = "Apple signing cleanup failed: " + "; ".join(failures)
            if exc_value is None:
                raise SigningError(message)
            print(message, file=sys.stderr)
        return False


def discover_macho_files(app: Path) -> list[Path]:
    """Use Apple's runner `file` tool to find Mach-O files without assuming an architecture."""
    if not app.is_dir() or app.suffix != ".app":
        raise SigningError(f"Expected an existing .app bundle: {app}")
    candidates = sorted(path for path in app.rglob("*") if path.is_file() and not path.is_symlink())
    found: list[Path] = []
    for path in candidates:
        result = _run(["file", "-b", path])
        if "Mach-O" in result.stdout:
            found.append(path)
    if not found:
        raise SigningError(f"No Mach-O code was found in {app}.")
    return found


def _plist(path: Path) -> dict:
    try:
        result = plistlib.loads(path.read_bytes())
    except (OSError, plistlib.InvalidFileException) as error:
        raise SigningError(f"Could not read property list {path}: {error}") from error
    if not isinstance(result, dict):
        raise SigningError(f"Property list must contain a dictionary: {path}")
    return result


def _bundle_executable(bundle: Path, macho_files: Sequence[Path]) -> Path | None:
    plist_paths = (
        bundle / "Contents" / "Info.plist",
        bundle / "Resources" / "Info.plist",
        bundle / "Versions" / "Current" / "Resources" / "Info.plist",
        bundle / "Info.plist",
    )
    executable_name = None
    for plist_path in plist_paths:
        if plist_path.is_file():
            executable_name = _plist(plist_path).get("CFBundleExecutable")
            if executable_name:
                break
    candidates = [path for path in macho_files if path.is_relative_to(bundle)]
    if executable_name:
        matching = [path for path in candidates if path.name == executable_name]
    else:
        matching = [path for path in candidates if path.name == bundle.stem]
    if len(matching) == 1:
        return matching[0]
    current = bundle / "Versions" / "Current"
    if current.exists():
        resolved_current = current.resolve()
        name = executable_name or bundle.stem
        matching = [path for path in candidates if path.parent.resolve() == resolved_current and path.name == name]
        if len(matching) == 1:
            return matching[0]
    if not candidates:
        return None
    raise SigningError(f"Could not identify the primary Mach-O executable in nested code bundle {bundle}.")


def _code_bundles(app: Path, macho_files: Sequence[Path]) -> list[tuple[Path, Path | None]]:
    bundles = []
    for path in app.rglob("*"):
        if path.is_dir() and path.suffix.lower() in CODE_BUNDLE_SUFFIXES:
            executable = _bundle_executable(path, macho_files)
            if executable is not None:
                bundles.append((path, executable))
    return sorted(bundles, key=lambda item: (len(item[0].parts), str(item[0])), reverse=True)


def _entitlement_for(name: str, directory: Path | None, *, required: bool) -> Path | None:
    if directory is None:
        if required:
            raise SigningError(f"Release entitlements are required for helper process {name}.")
        return None
    path = directory / f"{name}.entitlements"
    if not path.is_file():
        if required:
            raise SigningError(f"Missing release entitlements for helper process {name}: {path}")
        return None
    entitlements = _plist(path)
    disallowed = [
        key for key in (
            "com.apple.security.get-task-allow",
            "com.apple.security.cs.disable-library-validation",
            "com.apple.security.cs.allow-unsigned-executable-memory",
            "com.apple.security.cs.allow-dyld-environment-variables",
            "com.apple.security.cs.disable-executable-page-protection",
        ) if entitlements.get(key) is True
    ]
    unexpected = set(entitlements) - ENTITLEMENTS_ALLOWED
    if disallowed or unexpected:
        names = ", ".join(sorted(set(disallowed) | unexpected))
        raise SigningError(f"Unsafe or unaudited release entitlements for {name}: {names}")
    if "com.apple.security.cs.allow-jit" in entitlements:
        if name != "WebContent" or entitlements["com.apple.security.cs.allow-jit"] is not True:
            raise SigningError(f"JIT entitlement is only permitted for the Ladybird WebContent helper: {name}")
    if name == "WebContent" and entitlements.get("com.apple.security.cs.allow-jit") is not True:
        raise SigningError("The Ladybird WebContent release entitlements must enable its required JIT execution.")
    return path


def signing_plan(app: Path, entitlements_dir: Path | None = None) -> list[SigningTarget]:
    """Build an inside-out signing order, with the Elaris app bundle last."""
    app = app.resolve()
    macho_files = discover_macho_files(app)
    info = _plist(app / "Contents" / "Info.plist")
    executable_name = info.get("CFBundleExecutable")
    if not isinstance(executable_name, str) or not executable_name:
        raise SigningError("Elaris.app/Contents/Info.plist has no CFBundleExecutable.")
    main_executable = app / "Contents" / "MacOS" / executable_name
    if main_executable not in macho_files:
        raise SigningError(f"The main application executable is missing or is not Mach-O: {main_executable}")

    code_bundles = _code_bundles(app, macho_files)
    bundle_executables = {executable for _, executable in code_bundles}
    inner_targets = []
    for binary in macho_files:
        if binary == main_executable or binary in bundle_executables:
            continue
        helper = binary.parent == app / "Contents" / "MacOS"
        entitlement = _entitlement_for(binary.name, entitlements_dir, required=helper)
        inner_targets.append(SigningTarget(binary, entitlement))
    inner_targets.sort(key=lambda target: (0 if target.path.suffix == ".dylib" else 1, str(target.path)))

    bundle_targets = []
    for bundle, executable in code_bundles:
        helper = bundle.parent == app / "Contents" / "MacOS"
        entitlement = _entitlement_for(executable.name, entitlements_dir, required=helper)
        bundle_targets.append(SigningTarget(bundle, entitlement, is_bundle=True))
    main_entitlements = _entitlement_for(executable_name, entitlements_dir, required=False)
    return [*inner_targets, *bundle_targets, SigningTarget(app, main_entitlements, is_bundle=True)]


def _identity_for_team(keychain: Path, team_id: str) -> str:
    result = _run(["security", "find-identity", "-v", "-p", "codesigning", keychain])
    pattern = re.compile(
        r'^\s*\d+\)\s+([0-9A-F]{40,64})\s+"Developer ID Application: [^"]+\(([A-Z0-9]{10})\)"\s*$'
    )
    matches = [
        (match.group(1), match.group(2))
        for line in result.stdout.splitlines()
        if (match := pattern.match(line)) and match.group(2) == team_id
    ]
    if len(matches) != 1:
        raise SigningError(
            f"Expected exactly one valid Developer ID Application identity for team {team_id}; "
            f"found {len(matches)}. Check APPLE_TEAM_ID and the imported .p12."
        )
    return matches[0][0]


def _sign_target(target: SigningTarget, identity: str, keychain: Path) -> None:
    command: list[str | Path] = [
        "codesign", "--force", "--sign", identity, "--keychain", keychain,
        "--options", "runtime", "--timestamp",
    ]
    if target.entitlements is not None:
        command.extend(["--entitlements", target.entitlements])
    command.append(target.path)
    _run(command)


def _signature_details(path: Path, team_id: str, *, require_runtime: bool = True) -> str:
    _run(["codesign", "--verify", "--strict", "--verbose=2", path])
    result = _run(["codesign", "--display", "--verbose=4", path], check=False)
    details = result.stdout + result.stderr
    if result.returncode:
        raise SigningError(f"Could not inspect code signature: {path}")
    team = re.search(r"^TeamIdentifier=(.+)$", details, re.MULTILINE)
    if team is None or team.group(1).strip() != team_id:
        raise SigningError(f"Signature for {path} does not belong to Apple Developer team {team_id}.")
    flags = re.search(r"^CodeDirectory .*flags=.*$", details, re.MULTILINE)
    if require_runtime and (flags is None or "runtime" not in flags.group(0)):
        raise SigningError(f"Hardened Runtime is missing from {path}.")
    if "Timestamp=" not in details:
        raise SigningError(f"A secure signing timestamp is missing from {path}.")
    return details


def _signed_entitlements(path: Path) -> dict:
    # Ask for XML explicitly: modern codesign otherwise renders DER as human-readable text.
    # The plist goes to stdout; signature diagnostics on stderr are not part of it.
    result = _run(["codesign", "--display", "--entitlements", "-", "--xml", path], check=False)
    try:
        if result.returncode:
            raise ValueError("codesign failed")
        parsed = plistlib.loads(result.stdout.encode())
        if not isinstance(parsed, dict):
            raise ValueError("expected an entitlement dictionary")
    except (ExpatError, plistlib.InvalidFileException, ValueError) as error:
        raise SigningError(f"Could not read signed entitlements from {path}.") from error
    return parsed


def _verify_dependencies(app: Path, macho_files: Sequence[Path]) -> None:
    contents = app / "Contents"
    code_bundles = _code_bundles(app, macho_files)
    main_executable = app / "Contents" / "MacOS" / _plist(app / "Contents" / "Info.plist")["CFBundleExecutable"]
    runpath_cache: dict[Path, list[Path]] = {}

    def executable_for(binary: Path) -> Path:
        if binary.parent == app / "Contents" / "MacOS":
            return binary
        containers = [
            (bundle, executable)
            for bundle, executable in code_bundles
            if binary.is_relative_to(bundle)
        ]
        if containers:
            return max(containers, key=lambda item: len(item[0].parts))[1]
        return main_executable

    def runpaths_for(binary: Path) -> list[Path]:
        if binary in runpath_cache:
            return runpath_cache[binary]
        output = _run(["otool", "-l", binary]).stdout
        raw_runpaths: list[str] = []
        in_rpath = False
        for line in output.splitlines():
            if line.strip() == "cmd LC_RPATH":
                in_rpath = True
                continue
            if in_rpath:
                match = re.match(r"\s*path (.+) \(offset \d+\)$", line)
                if match:
                    raw_runpaths.append(match.group(1))
                    in_rpath = False
                elif line.strip().startswith("cmd "):
                    in_rpath = False

        expanded = []
        executable = executable_for(binary)
        for runpath in raw_runpaths:
            if runpath.startswith("@loader_path"):
                relative = runpath.removeprefix("@loader_path").lstrip("/")
                expanded.append((binary.parent / relative).resolve())
            elif runpath.startswith("@executable_path"):
                relative = runpath.removeprefix("@executable_path").lstrip("/")
                expanded.append((executable.parent / relative).resolve())
            elif runpath.startswith("/"):
                expanded.append(Path(runpath).resolve())
        runpath_cache[binary] = expanded
        return expanded

    for binary in macho_files:
        result = _run(["otool", "-L", binary])
        lines = result.stdout.splitlines()[1:]
        for line in lines:
            dependency = line.strip().split(" (compatibility version", 1)[0]
            if dependency.startswith(("/System/Library/", "/usr/lib/")):
                continue
            if dependency.startswith("@rpath/"):
                suffix = dependency.removeprefix("@rpath/")
                runpaths = list(dict.fromkeys([*runpaths_for(binary), *runpaths_for(executable_for(binary))]))
                candidates = [(runpath / suffix).resolve() for runpath in runpaths]
                if not any(
                    candidate.is_file()
                    and (
                        candidate.is_relative_to(contents)
                        or str(candidate).startswith(("/System/Library/", "/usr/lib/"))
                    )
                    for candidate in candidates
                ):
                    raise SigningError(f"{binary} has an unresolved bundled dependency: {dependency}")
                continue
            if dependency.startswith("@loader_path/"):
                resolved = (binary.parent / dependency.removeprefix("@loader_path/")).resolve()
                if not resolved.is_file():
                    raise SigningError(f"{binary} has an unresolved loader-relative dependency: {dependency}")
                continue
            if dependency.startswith("@executable_path/"):
                relative = dependency.removeprefix("@executable_path/")
                candidates = [contents / "MacOS" / relative]
                candidates.extend(
                    bundle / "Contents" / "MacOS" / relative
                    for bundle, _ in code_bundles
                    if bundle.suffix.lower() in {".app", ".xpc", ".appex"}
                )
                if not any(candidate.resolve().is_file() for candidate in candidates):
                    raise SigningError(f"{binary} has an unresolved executable-relative dependency: {dependency}")
                continue
            if dependency.startswith("/"):
                raise SigningError(f"{binary} links to an external non-system library: {dependency}")
            raise SigningError(f"{binary} has an unsupported library reference: {dependency}")


def verify_application(app: Path, team_id: str) -> None:
    """Verify every discovered Mach-O, nested code bundle, entitlement, and linked library."""
    app = app.resolve()
    macho_files = discover_macho_files(app)
    bundles = [SigningTarget(bundle, is_bundle=True) for bundle, _ in _code_bundles(app, macho_files)]
    for path in macho_files:
        _signature_details(path, team_id)
    for target in [*bundles, SigningTarget(app, is_bundle=True)]:
        if target.is_bundle:
            _signature_details(target.path, team_id)
    web_content = next((path for path in macho_files if path.name == "WebContent"), None)
    if web_content is not None:
        entitlements = _signed_entitlements(web_content)
        if entitlements.get("com.apple.security.cs.allow-jit") is not True:
            raise SigningError("The signed WebContent helper is missing its required JIT entitlement.")
        if any(entitlements.get(key) is True for key in (
            "com.apple.security.get-task-allow",
            "com.apple.security.cs.disable-library-validation",
            "com.apple.security.cs.allow-unsigned-executable-memory",
        )):
            raise SigningError("The signed WebContent helper contains an unsafe debug entitlement.")
    _verify_dependencies(app, macho_files)


def _verify_dmg_signature(dmg: Path, team_id: str) -> None:
    _run(["codesign", "--verify", "--strict", "--verbose=2", dmg])
    result = _run(["codesign", "--display", "--verbose=4", dmg], check=False)
    details = result.stdout + result.stderr
    team = re.search(r"^TeamIdentifier=(.+)$", details, re.MULTILINE)
    if result.returncode or team is None or team.group(1).strip() != team_id:
        raise SigningError(f"The disk image is not signed by Apple Developer team {team_id}.")
    if "Timestamp=" not in details:
        raise SigningError("The disk image signature does not contain a secure timestamp.")


def _notarize(dmg: Path, credentials: Credentials) -> str:
    result = _run(
        [
            "xcrun", "notarytool", "submit", dmg,
            "--apple-id", credentials.apple_id,
            "--team-id", credentials.team_id,
            "--password", credentials.app_password,
            "--wait", "--output-format", "json",
        ],
        check=False,
        quiet_failure=True,
        timeout=3600,
    )
    output = _safe_output(result.stdout, credentials)
    try:
        response = json.loads(output)
    except json.JSONDecodeError:
        response = {}
    submission_id = response.get("id") if isinstance(response, dict) else None
    status = response.get("status") if isinstance(response, dict) else None
    if result.returncode == 0 and status == "Accepted" and submission_id:
        return str(submission_id)

    diagnostic = ""
    if submission_id:
        log = _run(
            ["xcrun", "notarytool", "log", str(submission_id),
             "--apple-id", credentials.apple_id, "--team-id", credentials.team_id,
             "--password", credentials.app_password, "--output-format", "json"],
            check=False,
            quiet_failure=True,
            timeout=300,
        )
        diagnostic = _safe_output(log.stdout or log.stderr, credentials)
    if not diagnostic:
        diagnostic = _safe_output(result.stderr or result.stdout, credentials)
    if diagnostic:
        print("Apple notarization diagnostics:", file=sys.stderr)
        print(diagnostic, file=sys.stderr)
    raise SigningError(f"Apple notarization was not accepted (status: {status or 'submission failed'}).")


def _verify_mounted_image(dmg: Path, app_name: str, team_id: str) -> None:
    mountpoint = Path(tempfile.mkdtemp(prefix="elaris-notarized-dmg-"))
    mounted = False
    try:
        _run(["hdiutil", "attach", "-readonly", "-nobrowse", "-mountpoint", mountpoint, dmg])
        mounted = True
        mounted_app = mountpoint / app_name
        verify_application(mounted_app, team_id)
        _run(["spctl", "--assess", "--type", "execute", "--verbose=4", mounted_app])
    finally:
        try:
            if mounted:
                _run(["hdiutil", "detach", "-force", mountpoint])
        finally:
            shutil.rmtree(mountpoint, ignore_errors=True)


def _smoke_test_signed_application(app: Path) -> None:
    """Run the browser against the checked-in smoke page after Hardened Runtime signing."""
    info = _plist(app / "Contents" / "Info.plist")
    executable = info.get("CFBundleExecutable")
    if not isinstance(executable, str) or not executable:
        raise SigningError("The signed app has no main executable for its runtime smoke test.")
    smoke_page = ROOT / "bridge" / "tests" / "smoke.html"
    if not smoke_page.is_file():
        raise SigningError(f"The browser smoke page is missing: {smoke_page}")
    command = [
        app / "Contents" / "MacOS" / executable,
        smoke_page.as_uri(), "--", "--snap",
    ]
    child_environment = os.environ.copy()
    for name in REQUIRED_CREDENTIALS:
        child_environment.pop(name, None)
    with tempfile.TemporaryDirectory(prefix="elaris-signed-smoke-") as directory:
        screenshot = Path(directory) / "smoke.png"
        command.append(screenshot)
        log_path = Path(directory) / "browser.log"
        with log_path.open("w+") as log:
            process = subprocess.Popen(
                [str(part) for part in command],
                stdout=log,
                stderr=subprocess.STDOUT,
                env=child_environment,
                start_new_session=True,
            )
            failure = None
            deadline = time.monotonic() + 120
            while True:
                if screenshot.is_file() and screenshot.stat().st_size:
                    break
                status = process.poll()
                if status is not None:
                    failure = f"Signed browser smoke test exited before rendering (status {status})."
                    break
                if time.monotonic() >= deadline:
                    failure = "Signed browser smoke test did not render within 120 seconds."
                    break
                time.sleep(0.1)

            try:
                os.killpg(process.pid, signal.SIGTERM)
            except ProcessLookupError:
                pass
            try:
                process.wait(timeout=10)
            except subprocess.TimeoutExpired:
                try:
                    os.killpg(process.pid, signal.SIGKILL)
                except ProcessLookupError:
                    pass
                process.wait(timeout=10)
            if failure:
                log.flush()
                log.seek(0)
                detail = log.read().strip()
                if detail:
                    failure += f" Browser output: {detail}"
                raise SigningError(failure)


def _write_release_metadata(directory: Path, dmg: Path, team_id: str, submission_id: str) -> None:
    hasher = hashlib.sha256()
    with dmg.open("rb") as artifact:
        for block in iter(lambda: artifact.read(1024 * 1024), b""):
            hasher.update(block)
    digest = hasher.hexdigest()
    (directory / f"{dmg.name}.sha256").write_text(f"{digest}  {dmg.name}\n")
    (directory / "SIGNING-INFO.txt").write_text(
        "Status: notarized\n"
        f"Team ID: {team_id}\n"
        "Identity type: Developer ID Application\n"
        "Hardened Runtime: enabled for all Mach-O code\n"
        "Notarization: accepted\n"
        f"Submission ID: {submission_id}\n"
        "Stapling: validated\n"
    )
    build_info = directory / "BUILD-INFO.txt"
    if build_info.is_file():
        target = directory / f"{dmg.stem}.BUILD-INFO.txt"
        shutil.copy2(build_info, target)


def prepare(app: Path, entitlements_dir: Path) -> list[SigningTarget]:
    app = app.resolve()
    entitlements_dir = entitlements_dir.resolve()
    plan = signing_plan(app, entitlements_dir)
    for index, target in enumerate(plan, start=1):
        detail = f" ({target.entitlements.name})" if target.entitlements else ""
        print(f"{index:02d}. {target.path.relative_to(app.parent)}{detail}")
    print(f"Prepared {len(plan)} signing targets; credentials were not read or used.")
    return plan


def release(app: Path, entitlements_dir: Path, dmg: Path, volume_name: str) -> None:
    credentials = load_credentials()
    app = app.resolve()
    entitlements_dir = entitlements_dir.resolve()
    dmg = dmg.resolve()
    plan = signing_plan(app, entitlements_dir)
    dmg.parent.mkdir(parents=True, exist_ok=True)
    dmg.unlink(missing_ok=True)
    with TemporaryKeychain(credentials) as keychain:
        identity = _identity_for_team(keychain, credentials.team_id)
        print(f"Signing {len(plan)} targets with Developer ID Application for team {credentials.team_id}.")
        for target in plan:
            _sign_target(target, identity, keychain)
        verify_application(app, credentials.team_id)
        _smoke_test_signed_application(app)

        _run([ROOT / "packaging" / "macos" / "make-dmg.sh", app, dmg, volume_name])
        _run(["codesign", "--force", "--sign", identity, "--keychain", keychain, "--timestamp", dmg])
        _verify_dmg_signature(dmg, credentials.team_id)
        submission_id = _notarize(dmg, credentials)

        _run(["xcrun", "stapler", "staple", "-v", dmg])
        _run(["xcrun", "stapler", "validate", "-v", dmg])
        _verify_dmg_signature(dmg, credentials.team_id)
        _run(["spctl", "--assess", "--type", "open", "--verbose=4", dmg])
        _verify_mounted_image(dmg, app.name, credentials.team_id)
        _write_release_metadata(dmg.parent, dmg, credentials.team_id, submission_id)


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    commands = parser.add_subparsers(dest="command", required=True)
    prepare_parser = commands.add_parser("prepare", help="inspect the bundle and print a credential-free signing plan")
    prepare_parser.add_argument("--app", type=Path, required=True)
    prepare_parser.add_argument("--entitlements-dir", type=Path, required=True)
    release_parser = commands.add_parser("release", help="sign, notarize, staple, and verify a release DMG")
    release_parser.add_argument("--app", type=Path, required=True)
    release_parser.add_argument("--entitlements-dir", type=Path, required=True)
    release_parser.add_argument("--dmg", type=Path, required=True)
    release_parser.add_argument("--volume-name", default="Elaris")
    args = parser.parse_args()
    try:
        if args.command == "prepare":
            prepare(args.app, args.entitlements_dir)
        else:
            release(args.app, args.entitlements_dir, args.dmg, args.volume_name)
    except (OSError, SigningError, subprocess.TimeoutExpired) as error:
        print(f"macOS release signing failed: {error}", file=sys.stderr)
        return 1
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
