# Minimal plain-HTTP forward proxy so the L16 (no internet) can reach package mirrors.
# Runs on the PC at 127.0.0.1:3128; the phone reaches it through `ssh -R 3128:127.0.0.1:3128`.
# Only allows the two mirrors we install from.
import http.server, socketserver, urllib.request, urllib.parse

ALLOWED = {'dl-cdn.alpinelinux.org', 'mirror.postmarketos.org'}


class Proxy(http.server.BaseHTTPRequestHandler):
    def do_GET(self):
        host = urllib.parse.urlsplit(self.path).hostname
        if host not in ALLOWED:
            self.send_error(403)
            return
        try:
            with urllib.request.urlopen(self.path, timeout=60) as r:
                self.send_response(r.status)
                for k in ('Content-Type', 'Content-Length', 'Last-Modified', 'ETag'):
                    if r.headers.get(k):
                        self.send_header(k, r.headers[k])
                self.end_headers()
                while chunk := r.read(65536):
                    self.wfile.write(chunk)
        except urllib.error.HTTPError as e:
            self.send_error(e.code)

    def log_message(self, fmt, *args):
        print(fmt % args, flush=True)


class Server(socketserver.ThreadingMixIn, http.server.HTTPServer):
    daemon_threads = True


Server(('127.0.0.1', 3128), Proxy).serve_forever()
