#!/usr/bin/env python3
import argparse
import pathlib
import re
import sys


def parse_target(prompt: str) -> str | None:
    match = re.search(r"You are reviewing: \*\*`([^`]+)`\*\*", prompt)
    return match.group(1) if match else None


def append_line(path: pathlib.Path, line: str) -> None:
    existing = path.read_text() if path.exists() else ""
    if line not in existing:
        if existing and not existing.endswith("\n"):
            existing += "\n"
        existing += line + "\n"
        path.write_text(existing)


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--mode", required=True)
    parser.add_argument("--prompt", required=True)
    parser.add_argument("--cwd", required=True)
    parser.add_argument("--log", required=True)
    parser.add_argument("--other", default="")
    args = parser.parse_args()

    cwd = pathlib.Path(args.cwd)
    target = parse_target(args.prompt)
    target_path = cwd / target if target else None
    log_path = pathlib.Path(args.log)
    log_path.write_text(f"fake-agent mode={args.mode}\n")

    if args.mode == "no-op":
        return 0

    if args.mode == "one-pass":
        state = cwd / ".fake-agent-once"
        if not state.exists() and target_path is not None:
            append_line(target_path, "- fake-agent: one-pass refinement")
            state.write_text("done\n")
        return 0

    if args.mode == "oscillate" and target_path is not None:
        text = target_path.read_text()
        marker = "- fake-agent: oscillate"
        if marker in text:
            target_path.write_text(text.replace(marker + "\n", "").replace(marker, ""))
        else:
            append_line(target_path, marker)
        return 0

    if args.mode == "ripple" and target_path is not None:
        append_line(target_path, "- fake-agent: ripple primary")
        if args.other:
            append_line(cwd / args.other, "- fake-agent: ripple secondary")
        return 0

    if args.mode == "sweep" and target_path is not None:
        sweep_root = cwd / target
        for path in sorted(sweep_root.rglob("*.md")):
            append_line(path, f"- fake-agent: sweep {path.name}")
        return 0

    print(f"unsupported fake-agent mode: {args.mode}", file=sys.stderr)
    return 1


if __name__ == "__main__":
    raise SystemExit(main())
