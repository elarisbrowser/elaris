# macOS code signing and notarization

Official macOS releases use a Developer ID Application certificate and Apple's notarization service. The signing job runs in the protected GitHub Actions `apple-signing` environment after Elaris and Ladybird have been built, bundled, and smoke-tested. Pull requests and the default manual test workflow do not receive Apple credentials.

## Bundle audit and signing policy

The fresh release bundle contains 94 Mach-O files: the `elaris` executable, 11 Ladybird helper executables, and 82 dynamic libraries. The helpers are `AudioServer`, `Compositor`, `Hub`, `ImageDecoder`, `MediaServer`, `ProcessReaper`, `RequestServer`, `SystemProbe`, `VideoDecoderServer`, `WebContent`, and `WebRTCClient`. The inspected release layout has no embedded frameworks, XPC bundles, or plug-ins; the signing tool discovers Mach-O code and nested code bundles from the bundle on each run, so it does not assume a fixed CPU architecture or fixed library count.

This inventory was verified by running the existing library-copy and verification steps on a temporary app copy, using the already-built Ladybird outputs. The native engine was not rebuilt.

The signing order is dynamic libraries and standalone executables first, nested code bundles from deepest to shallowest, and `Elaris.app` last. The tool verifies each discovered Mach-O and code bundle individually, then checks linked libraries with `otool`. It does not use `codesign --deep` to sign code.

Release entitlements come from Ladybird's generated `*.release.entitlements` files. For the inspected pinned build, `WebContent` alone has `com.apple.security.cs.allow-jit`; this is required for Ladybird's JavaScript JIT under Hardened Runtime. The other release helper entitlement files are empty. Production signing rejects `get-task-allow`, disabled library validation, unsigned executable memory, DYLD environment access, executable page protection exceptions, and unreviewed entitlement keys. Library validation stays enabled because Elaris and its bundled libraries are signed by the same Developer ID team. The debug entitlement files are not used for release signing.

Apple documents the [Hardened Runtime and notarization requirements](https://developer.apple.com/documentation/security/notarizing-macos-software-before-distribution), the [JIT entitlement for Apple silicon](https://developer.apple.com/documentation/apple-silicon/porting-just-in-time-compilers-to-apple-silicon), and [common notarization failures](https://developer.apple.com/documentation/security/resolving-common-notarization-issues).

## Prepare credentials

An active Apple Developer Program membership is required.

1. As the Account Holder, open **Certificates, Identifiers & Profiles → Certificates → Add** and create a **Developer ID Application** certificate. Apple describes the steps in [Create Developer ID certificates](https://developer.apple.com/help/account/certificates/create-developer-id-certificates).
2. If creating the certificate manually, create a Certificate Signing Request in Keychain Access, upload it, download the resulting `.cer`, and open it to add it to the keychain. Confirm that **My Certificates** shows the Developer ID Application certificate together with its private key.
3. In Keychain Access, select the Developer ID Application identity under **My Certificates**, choose **Export Items**, and export it as a `.p12`. Protect the export with a strong, unique password. The `.p12` must contain the certificate and its private key.
4. Base64-encode the `.p12` locally without adding it to the repository. For example, on macOS:

   ```sh
   base64 -i DeveloperID.p12 | tr -d '\n' > DeveloperID.p12.base64
   ```

   Keep the temporary files private and remove them after configuring the secret. Do not paste the encoded value into a terminal command, commit it, or attach it to an issue.
5. At [account.apple.com](https://account.apple.com/), create an app-specific password under **Sign-In and Security → App-Specific Passwords**. Use this password for notarization; do not use the primary Apple Account password. Apple requires two-factor authentication for app-specific passwords. See [Apple's app-specific password guide](https://support.apple.com/en-us/102654).

## Configure GitHub Actions

Create a repository environment named `apple-signing`. Restrict deployments to the `main` branch and `v*` tags, and configure required reviewers if the repository plan supports that policy. The signing job is the only job that uses this environment. GitHub withholds environment secrets until configured protection rules pass; see [GitHub environment protection](https://docs.github.com/en/actions/how-tos/deploy/configure-and-manage-deployments/manage-environments).

Add these **environment secrets**:

| Secret | Value |
| --- | --- |
| `APPLE_CERTIFICATE` | Base64-encoded `.p12` containing the Developer ID Application identity and private key |
| `APPLE_CERTIFICATE_PASSWORD` | Password used when exporting the `.p12` |
| `APPLE_TEAM_ID` | 10-character Apple Developer team identifier |
| `APPLE_ID` | Apple Account email used for notarization |
| `APPLE_APP_PASSWORD` | Apple app-specific password |

On a tag release or trusted canary run, the workflow requires all five values and fails before producing the signed artifact if any value is missing or invalid. It creates a temporary runner keychain, imports the identity with access limited to Apple's signing tools, and removes the keychain and temporary certificate file even when a signing or notarization step fails. Credentials are passed only to the signing step, removed from child-process environments, and never printed or written into the repository or release artifacts.

The reusable release workflow obtains these secrets from the `apple-signing` environment. The nightly caller is restricted to `main`; the reusable workflow checks that its ref is `main`, and version tags must point to commits contained in `main`. The `workflow_dispatch` default is `test`: it runs smoke tests and signing-plan validation, then produces a clearly named unsigned test DMG without secrets. Selecting `release` manually is allowed only from `main` and uses the protected environment; it does not publish a GitHub release. Only version tags and the trusted canary workflow publish release assets.

## Run and inspect the pipeline

The macOS release sequence restores the existing compiler and Ladybird caches, builds and bundles Elaris, runs Ladybird smoke tests, stages the generated release entitlements, and prints a credential-free signing plan. The assembled app and entitlement files are transferred to a separate, one-day signing job. That job signs nested code, signs the app bundle last, verifies signatures and dependencies, runs the signed browser against the existing HTML smoke page and confirms that it renders, packages the existing DMG, signs and submits the DMG with `notarytool`, waits for Apple's result, retrieves the notary log on rejection, staples and validates the accepted ticket, then runs Gatekeeper and mounted-app checks. It generates the checksum only after stapling and verification.

To inspect a locally built bundle without credentials:

```sh
mkdir -p build/signing-entitlements
for source in build/cpp-ladybird/Services/*/*.release.entitlements; do
  [ -f "$source" ] || continue
  name="$(basename "$source" .release.entitlements)"
  cp "$source" "build/signing-entitlements/$name.entitlements"
done
python3 packaging/macos/signing.py prepare \
  --app build/Elaris.app \
  --entitlements-dir build/signing-entitlements
```

The plan reports the code paths and entitlement files that would be used. It does not read credentials or alter the app. The workflow's unit tests mock signing and notarization to cover failures without a Developer account. They cannot prove Apple's service will accept Elaris; the first credentialed release run after membership activation is the required end-to-end check.

## Rotate or revoke credentials

To rotate a certificate, create a replacement Developer ID Application identity, export a new `.p12`, update the certificate and password secrets together, and run a signed release from trusted `main` before revoking the old identity. Revoke a compromised certificate promptly in the Apple Developer account; Apple notes that apps signed with a revoked Developer ID certificate can no longer be installed or launched. Revoke the Apple app-specific password at account.apple.com and replace `APPLE_APP_PASSWORD`. If the primary Apple Account password is changed or reset, Apple revokes its app-specific passwords automatically.

## Troubleshoot notarization

- The workflow fails before signing if a required secret is missing, the `.p12` cannot be imported, the identity is not a Developer ID Application certificate for `APPLE_TEAM_ID`, or multiple matching identities are found.
- A missing WebContent JIT entitlement, debug entitlement, invalid nested signature, missing timestamp, or external library reference fails local verification before notarization.
- If Apple rejects a submission, the signing tool fetches and prints the `notarytool log` diagnostics before failing. Fix every reported signature, entitlement, or binary issue, then rerun from trusted `main` or push a corrected release tag. Apple recommends reviewing the log even for accepted submissions; retain the Actions run log alongside release review.
- If stapling or Gatekeeper validation fails, the job fails and the signed release artifact is not uploaded. Check the Xcode selection and runner network access to Apple's timestamp and notarization services before retrying.

Apple's [notarytool workflow guide](https://developer.apple.com/documentation/security/customizing-the-notarization-workflow) covers submissions and log retrieval. [Resolving common notarization issues](https://developer.apple.com/documentation/security/resolving-common-notarization-issues) explains common signature and entitlement errors.

## Verify a downloaded release

From the directory containing the downloaded DMG and checksum:

Replace `VERSION` below with the version in the downloaded file name.

```sh
shasum -a 256 -c Elaris-VERSION-macos-arm64.dmg.sha256
xcrun stapler validate -v Elaris-VERSION-macos-arm64.dmg
codesign --verify --strict --verbose=2 Elaris-VERSION-macos-arm64.dmg
spctl --assess --type open --verbose=4 Elaris-VERSION-macos-arm64.dmg
```

After mounting the DMG, verify the app before opening it:

```sh
codesign --verify --strict --verbose=2 /Volumes/Elaris/Elaris.app
spctl --assess --type execute --verbose=4 /Volumes/Elaris/Elaris.app
```

The release signer separately enumerates and verifies each Mach-O and nested code bundle; it never relies on recursive signing.
