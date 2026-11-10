package deploy

import "github.com/homelab-selfuryon/platform-kit/app"

app.#App & {
	name: "notes-stats"
	team: "notes"
	port: 8080
	size: "small"
	env: LOG_LEVEL: "info"
	// Not public. It calls notes-api: egress to it and a projected token (audience notes-api) at
	// /var/run/secrets/tokens/notes-api/token; NOTES_API_URL is set by the platform.
	network: calls: [{app: "notes-api"}]
}
