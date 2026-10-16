#!/usr/bin/env python3
"""Generates a synthetic Git repository with about N lines of Rust, TypeScript and Python code.

Usage: generate.py <target-folder> [lines]   (default 100000 lines)
"""
import os
import subprocess
import sys

TARGET = sys.argv[1]
LINES = int(sys.argv[2]) if len(sys.argv) > 2 else 100_000
FILE_LINES = 200


def rust_file(index):
    out = [f"/// Module {index}.", f"pub struct Engine{index} {{ pub value: u64 }}", f"impl Engine{index} {{"]
    for fn in range(30):
        out += [f"    /// Function {fn}.", f"    pub fn compute_{index}_{fn}(&self, input: u64) -> u64 {{",
                f"        input.wrapping_mul({fn + 3}) ^ self.value", "    }"]
    out.append("}")
    return out


def ts_file(index):
    out = [f"export class Widget{index} {{"]
    for fn in range(30):
        out += [f"  renderPart{index}x{fn}(size: number): number {{", f"    return size * {fn + 2};", "  }", ""]
    out.append("}")
    return out


def py_file(index):
    out = ["import math", "", f"class Report{index}:"]
    for fn in range(30):
        out += [f"    def summarize_{index}_{fn}(self, values):", f"        return math.fsum(values) * {fn + 1}", ""]
    return out


def main():
    os.makedirs(TARGET, exist_ok=True)
    written, index = 0, 0
    makers = [("src", "rs", rust_file), ("web", "ts", ts_file), ("tools", "py", py_file)]
    while written < LINES:
        folder, ext, make = makers[index % 3]
        path = os.path.join(TARGET, folder, f"part{index // 50}", f"file_{index}.{ext}")
        os.makedirs(os.path.dirname(path), exist_ok=True)
        lines = make(index)
        with open(path, "w") as handle:
            handle.write("\n".join(lines) + "\n")
        written += len(lines)
        index += 1
    env = dict(os.environ, GIT_AUTHOR_NAME="bench", GIT_AUTHOR_EMAIL="b@b", GIT_COMMITTER_NAME="bench", GIT_COMMITTER_EMAIL="b@b")
    subprocess.run(["git", "-C", TARGET, "init", "-q"], check=True)
    subprocess.run(["git", "-C", TARGET, "add", "-A"], check=True)
    subprocess.run(["git", "-C", TARGET, "commit", "-q", "-m", "corpus"], check=True, env=env)
    print(f"{index} files, {written} lines in {TARGET}")


main()
