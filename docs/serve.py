#!/usr/bin/env python3
import argparse
import errno
import functools
import http.server
import pathlib
import sys
import threading
import webbrowser

ROOT = pathlib.Path(__file__).resolve().parent

TYPES = {
    ".html": "text/html; charset=utf-8",
    ".md": "text/markdown; charset=utf-8",
    ".js": "text/javascript; charset=utf-8",
    ".css": "text/css; charset=utf-8",
    ".svg": "image/svg+xml",
    ".json": "application/json",
    ".png": "image/png",
    ".jpg": "image/jpeg",
    ".gif": "image/gif",
    ".webp": "image/webp",
    ".ico": "image/x-icon",
}


class Handler(http.server.SimpleHTTPRequestHandler):
    extensions_map = {**http.server.SimpleHTTPRequestHandler.extensions_map, **TYPES}

    def end_headers(self):
        self.send_header("Cache-Control", "no-store")
        super().end_headers()

    def log_message(self, format, *args):
        if self.quiet:
            return
        super().log_message(format, *args)


def open_server(port, tries):
    for offset in range(tries):
        try:
            return http.server.ThreadingHTTPServer(("127.0.0.1", port + offset), functools.partial(Handler, directory=str(ROOT)))
        except OSError as error:
            if error.errno not in (errno.EADDRINUSE, getattr(errno, "WSAEADDRINUSE", -1), 10013):
                raise
    raise SystemExit(f"No free port found between {port} and {port + tries - 1}.")


def main():
    parser = argparse.ArgumentParser(description="Preview the luv docs on this computer.")
    parser.add_argument("--port", type=int, default=8000, help="first port to try (default 8000)")
    parser.add_argument("--no-browser", action="store_true", help="do not open a browser")
    parser.add_argument("--quiet", action="store_true", help="hide the request log")
    options = parser.parse_args()
    Handler.quiet = options.quiet
    server = open_server(options.port, 20)
    url = f"http://localhost:{server.server_address[1]}/"
    print(f"Serving the docs at {url}")
    print("Edit any Markdown file and refresh the page to see it.")
    print("Press Ctrl+C to stop.")
    if not options.no_browser:
        threading.Timer(0.4, webbrowser.open, args=(url,)).start()
    try:
        server.serve_forever()
    except KeyboardInterrupt:
        print("\nStopped.")
    finally:
        server.server_close()
    return 0


if __name__ == "__main__":
    sys.exit(main())
