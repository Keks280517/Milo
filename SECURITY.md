# Security Policy

Milo runs on your PC with access to your files, apps and browser, so security reports are taken seriously.

## Reporting a vulnerability

Please **do not open a public issue** for security problems. Use GitHub's private reporting instead:
**Security** tab → **Report a vulnerability**.

Include what you found, how to reproduce it and which version you used (Settings → System shows it). You'll get an answer as soon as possible; this is a personal project, so please allow a few days.

## Supported versions

Only the latest release receives fixes.

## What Milo keeps private by design

- API keys live only in the Windows Credential Manager, never in files or logs.
- Speech recognition and the local AI (Ollama / LM Studio) run entirely on your PC.
- Milo never approves Claude Code permission requests on its own.
