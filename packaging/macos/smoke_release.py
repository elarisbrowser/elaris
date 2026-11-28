#!/usr/bin/env python3
"""Smoke-test a bundled Elaris app before any release credentials are used."""

import argparse
import sys
from pathlib import Path

from signing import SigningError, _smoke_test_signed_application


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--app", type=Path, required=True)
    args = parser.parse_args()
    try:
        # Runtime tracing is identical for an ad-hoc and a Developer ID signature.
        _smoke_test_signed_application(args.app.resolve())
    except (OSError, SigningError) as error:
        print(f"Packaged browser smoke test failed: {error}", file=sys.stderr)
        return 1
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
