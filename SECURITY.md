# Security policy

## Reporting a vulnerability

**Please do not open a public GitHub issue for a security vulnerability.**
Issues are public immediately; a vulnerability report needs to stay private
until there's a fix.

Use GitHub's **private vulnerability reporting** instead: go to this repo's
**Security** tab → **Report a vulnerability**. That opens a private draft
advisory visible only to the maintainer and you, with its own thread for
follow-up questions — no public disclosure until it's resolved.

*(If that option isn't available on this repo yet, it needs enabling once
under Settings → Security → "Private vulnerability reporting" — from a
reporter's side, if you don't see the option, the maintainer hasn't turned
it on. Open a minimal, non-specific issue asking for a private reporting
channel instead of describing the vulnerability itself.)*

## What to include

Enough to reproduce: affected version/commit, steps, and the impact as you
understand it (what an attacker could actually do). A proof-of-concept
helps but isn't required to open the report.

## Scope

Librarium is self-hosted software. Most deployments run in a trust boundary
the operator controls (a home network, a private server) — vulnerabilities
that matter most are the ones that break that boundary:

- Authentication/authorization bypass (JWT, API keys, session handling,
  vault sharing/roles)
- Path traversal or arbitrary file access outside a vault's directory
- Injection (SQL, command, or otherwise) reachable from note content or the
  API
- Anything that lets an unauthenticated request do more than the code
  intends when `auth.enabled = false` on a *loopback-only* bind (the
  documented safe default — see [docs/CONFIGURATION.md](docs/CONFIGURATION.md))

Out of scope: issues that require an attacker to already have valid
credentials for the account being attacked, and denial-of-service reports
against a self-hosted single-tenant service (rate limiting exists but isn't
the focus of this policy).

## Supported versions

Pre-1.0: only the latest released version is supported. There is no
long-term-support branch yet.
