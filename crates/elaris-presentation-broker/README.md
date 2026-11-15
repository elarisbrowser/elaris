<p align="center"><img src="../elaris-brand/assets/logo.svg" alt="Photon" width="72" /></p>

# `elaris-presentation-broker`

The macOS XPC service executable for Photon’s native presentation transport. It is a small process entry point; the XPC protocol and service implementation live in [`elaris-presentation-ipc`](../elaris-presentation-ipc/README.md).

## Use

This binary is built and launched as part of Photon’s macOS presentation setup. It is not a browser UI crate or a standalone service to embed in another app. See the [native GPU presentation guide](../../Documentation/NativeGpuPresentation.md) for the transport’s role.
