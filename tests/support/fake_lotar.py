#!/usr/bin/env python3
"""Stands in for the lotar CLI: lists comments from FAKE_LOTAR_COMMENTS, logs the rest to FAKE_LOTAR_LOG."""
import json
import os
import pathlib
import sys


def main() -> int:
    args = sys.argv[1:]
    if args[:3] == ["--format", "json", "comment"] and len(args) == 4:
        comments = os.environ.get("FAKE_LOTAR_COMMENTS")
        print(pathlib.Path(comments).read_text() if comments else '{"items": []}')
        return 0
    if args[:1] == ["comment"]:
        with open(os.environ["FAKE_LOTAR_LOG"], "a") as log:
            log.write(json.dumps(args) + "\n")
        return 0
    print(f"unsupported fake-lotar call: {args}", file=sys.stderr)
    return 1


if __name__ == "__main__":
    raise SystemExit(main())
