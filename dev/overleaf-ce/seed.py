#!/usr/bin/env python3
"""Set up the local Overleaf CE in compose.yml for Canopy's sync tests.

    python dev/overleaf-ce/seed.py --env   # write .env with a random secret (before `up`)
    python dev/overleaf-ce/seed.py         # test user, sign-in, sample projects

Writes dev/overleaf-ce/.test-user.json (gitignored): the throwaway user's
email and password, a signed-in session cookie, and the sample project ids.
The integration tests read it. It only ever talks to http://localhost:8090.
Standard library only.
"""
import base64
import http.cookiejar
import json
import os
import re
import secrets
import subprocess
import sys
import urllib.error
import urllib.request

HERE = os.path.dirname(os.path.abspath(__file__))
BASE = "http://localhost:8090"
EMAIL = "canopy@test.local"
STATE = os.path.join(HERE, ".test-user.json")
ENV = os.path.join(HERE, ".env")
CONTAINER = "canopy-overleaf"


def write_env():
    if os.path.exists(ENV):
        print(".env already exists")
        return
    secret = base64.b64encode(secrets.token_bytes(32)).decode()
    with open(ENV, "w", encoding="ascii", newline="\n") as fh:
        fh.write(f"OVERLEAF_INVITE_TOKEN_SECRET={secret}\n")
    print("wrote .env")


class Http:
    def __init__(self):
        self.jar = http.cookiejar.CookieJar()
        self.opener = urllib.request.build_opener(urllib.request.HTTPCookieProcessor(self.jar))

    def get(self, path):
        with self.opener.open(BASE + path) as r:
            return r.read().decode("utf-8", "replace")

    def post(self, path, body, csrf):
        req = urllib.request.Request(
            BASE + path,
            data=json.dumps(body).encode(),
            headers={"Content-Type": "application/json", "Accept": "application/json", "X-Csrf-Token": csrf},
            method="POST",
        )
        try:
            with self.opener.open(req) as r:
                text = r.read().decode("utf-8", "replace")
                return r.status, (json.loads(text) if text.strip().startswith("{") else text)
        except urllib.error.HTTPError as e:
            return e.code, e.read().decode("utf-8", "replace")

    def csrf(self, path):
        page = self.get(path)
        m = re.search(r'name="ol-csrfToken" content="([^"]+)"', page)
        if not m:
            sys.exit(f"no CSRF token on {path}")
        return m.group(1)

    def cookie_header(self):
        return "; ".join(f"{c.name}={c.value}" for c in self.jar)


def create_user():
    out = subprocess.run(
        ["docker", "exec", CONTAINER, "bash", "-c",
         f"cd /overleaf/services/web && node modules/server-ce-scripts/scripts/create-user.mjs --admin --email={EMAIL}"],
        capture_output=True, text=True,
    )
    m = re.search(r"(/user/activate\?token=[^\s&]+&user_id=[0-9a-f]+)", out.stdout + out.stderr)
    if not m:
        sys.exit("create-user didn't print an activation link:\n" + (out.stdout + out.stderr)[-2000:])
    return m.group(1)


def main():
    if "--env" in sys.argv:
        write_env()
        return 0
    state = json.load(open(STATE)) if os.path.exists(STATE) else {}
    s = Http()
    if "password" not in state:
        link = create_user()
        password = secrets.token_urlsafe(18)
        token = re.search(r"token=([^&]+)", link).group(1)
        csrf = s.csrf(link)
        status, body = s.post("/user/password/set", {"passwordResetToken": token, "password": password}, csrf)
        if status >= 400:
            sys.exit(f"setting the password failed: {status} {body}")
        state = {"email": EMAIL, "password": password}
        print("created", EMAIL)

    s = Http()
    csrf = s.csrf("/login")
    status, body = s.post("/login", {"email": state["email"], "password": state["password"]}, csrf)
    if status >= 400:
        sys.exit(f"sign-in failed: {status} {body}")
    state["cookie"] = s.cookie_header()

    projects = state.get("projects", {})
    csrf = s.csrf("/project")
    for key, name, template in [("example", "Canopy sync test", "example"), ("blank", "Canopy blank test", "none")]:
        if key in projects:
            continue
        status, body = s.post("/project/new", {"projectName": name, "template": template}, csrf)
        if status >= 400 or not isinstance(body, dict):
            sys.exit(f"creating {name} failed: {status} {body}")
        projects[key] = body["project_id"]
        print("created project", name)
    state["projects"] = projects

    with open(STATE, "w", encoding="utf-8", newline="\n") as fh:
        json.dump(state, fh, indent=2)
    print("wrote", os.path.basename(STATE))
    return 0


if __name__ == "__main__":
    sys.exit(main())
