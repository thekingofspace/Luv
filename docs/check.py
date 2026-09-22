#!/usr/bin/env python3
import pathlib
import re
import sys

ROOT = pathlib.Path(__file__).resolve().parent
PAGES = ROOT / "pages"
SIDEBAR = ROOT / "sidebar.md"

ALLOWED_HYPHENS = {"SPIR-V", "UTF-8", "UTF-16", "P-256", "P-384"}

FENCE = re.compile(r"^ {0,3}(`{3,}|~{3,})\s*([^\s`]*)")
HEADING = re.compile(r"^ {0,3}(#{1,6})\s+(.*?)\s*$")
INLINE_CODE = re.compile(r"(`+)(.*?)\1")
LINK_TARGET = re.compile(r"\]\(([^)]*)\)")
URL = re.compile(r"<?https?://[^\s>)]+>?")
HYPHENATED = re.compile(r"(?<![\w./@-])[A-Za-z][A-Za-z0-9]*(?:-[A-Za-z0-9]+)+(?![\w/.-]*\.\w)")
STRING = re.compile(r"\"(?:\\.|[^\"\\])*\"|'(?:\\.|[^'\\])*'|`(?:\\.|[^`\\])*`")

COMMENTS = {
    "luau": re.compile(r"--"),
    "lua": re.compile(r"--"),
    "c": re.compile(r"(^|\s)(//|/\*)"),
    "rust": re.compile(r"(^|\s)(//|/\*)"),
    "wgsl": re.compile(r"(^|\s)(//|/\*)"),
    "glsl": re.compile(r"(^|\s)(//|/\*)"),
    "toml": re.compile(r"(^|\s)#"),
    "yaml": re.compile(r"(^|\s)#"),
    "shell": re.compile(r"(^|\s)#"),
    "sh": re.compile(r"(^|\s)#"),
    "bash": re.compile(r"(^|\s)#"),
    "powershell": re.compile(r"(^|\s)#"),
}


def slug(text):
    text = re.sub(r"!\[([^\]]*)\]\([^)]*\)", r"\1", text)
    text = re.sub(r"\[([^\]]*)\]\([^)]*\)", r"\1", text)
    text = re.sub(r"[`*]", "", text).strip().lower()
    return re.sub(r"[^a-z0-9]+", "-", text).strip("-") or "section"


def page_id(path):
    return path.relative_to(PAGES).with_suffix("").as_posix()


def walk(path):
    fence = None
    language = ""
    for number, line in enumerate(path.read_text(encoding="utf-8").splitlines(), 1):
        opener = FENCE.match(line)
        if fence:
            if opener and opener.group(1)[0] == fence[0] and len(opener.group(1)) >= len(fence) and not opener.group(2):
                fence = None
                yield number, line, "fence-end", language
                continue
            yield number, line, "code", language
            continue
        if opener:
            fence = opener.group(1)
            language = (opener.group(2) or "text").lower()
            yield number, line, "fence-start", language
            continue
        yield number, line, "prose", ""


def collect_anchors(pages):
    anchors = {}
    for path in pages:
        seen = {}
        found = set()
        for _, line, kind, _ in walk(path):
            if kind != "prose":
                continue
            heading = HEADING.match(line)
            if heading:
                base = slug(heading.group(2))
                count = seen.get(base, 0)
                seen[base] = count + 1
                found.add(base if not count else f"{base}-{count + 1}")
        anchors[page_id(path)] = found
    return anchors


def resolve(folder, target):
    parts = [] if target.startswith("/") else list(folder)
    for piece in target.lstrip("/").split("/"):
        if piece == "..":
            if parts:
                parts.pop()
        elif piece and piece != ".":
            parts.append(piece)
    return "/".join(parts)


def check_links(page, number, line, folder, existing, anchors, problems):
    for raw in LINK_TARGET.findall(line):
        target = raw.split()[0] if raw.split() else raw
        if re.match(r"^[a-z][a-z0-9+.-]*:", target) or target.startswith("//"):
            continue
        path, _, hash_part = target.partition("#")
        if path.endswith(".md"):
            linked = resolve(folder, path)[:-3]
            if linked not in existing:
                problems.append((page, number, f"link to a missing page: {target}"))
            elif hash_part and hash_part not in anchors.get(linked, set()):
                problems.append((page, number, f"link to a missing heading: {target}"))
        elif not path and hash_part:
            if hash_part not in anchors.get(page, set()):
                problems.append((page, number, f"link to a missing heading: {target}"))
        elif path:
            file = (ROOT / resolve(["pages", *folder], path)) if not path.startswith("/") else ROOT / path.lstrip("/")
            if not file.exists():
                problems.append((page, number, f"link to a missing file: {target}"))


def check_prose(page, number, line, problems):
    prose = INLINE_CODE.sub("", line)
    prose = LINK_TARGET.sub("]()", prose)
    prose = URL.sub("", prose)
    if "\u2014" in prose or "\u2013" in prose:
        problems.append((page, number, "uses a long dash"))
    if ";" in re.sub(r"&[a-z]+;|&#\d+;", "", prose):
        problems.append((page, number, "uses a semicolon outside of code"))
    for word in HYPHENATED.findall(prose):
        if word not in ALLOWED_HYPHENS:
            problems.append((page, number, f"uses a hyphenated word: {word}"))


def check_code(page, number, line, language, problems):
    rule = COMMENTS.get(language)
    if rule and rule.search(STRING.sub('""', line)):
        problems.append((page, number, f"has a comment in {language} code: {line.strip()}"))


def main():
    problems = []
    pages = sorted(PAGES.rglob("*.md"))
    existing = {page_id(path) for path in pages}
    listed = [
        re.sub(r"^(\./)?pages/", "", target)[:-3]
        for target in re.findall(r"\]\(([^)\s]+\.md)\)", SIDEBAR.read_text(encoding="utf-8"))
    ]
    for missing in sorted(set(listed) - existing):
        problems.append(("sidebar", 0, f"lists a missing page: {missing}"))
    for extra in sorted(existing - set(listed)):
        problems.append(("sidebar", 0, f"does not list the page: {extra}"))
    for duplicate in sorted({entry for entry in listed if listed.count(entry) > 1}):
        problems.append(("sidebar", 0, f"lists a page twice: {duplicate}"))
    anchors = collect_anchors(pages)
    for path in pages:
        page = page_id(path)
        folder = page.split("/")[:-1]
        first = next((line for _, line, kind, _ in walk(path) if line.strip()), "")
        if not first.startswith("# "):
            problems.append((page, 1, "does not start with a # title"))
        for number, line, kind, language in walk(path):
            if kind == "prose":
                check_links(page, number, line, folder, existing, anchors, problems)
                check_prose(page, number, line, problems)
            elif kind == "code":
                check_code(page, number, line, language, problems)
    for page, number, message in problems:
        where = f"{page}.md:{number}" if number else f"{page}.md"
        print(f"{where}: {message}")
    print(f"Checked {len(pages)} pages, found {len(problems)} problem{'s' if len(problems) != 1 else ''}.")
    return 1 if problems else 0


if __name__ == "__main__":
    sys.exit(main())
