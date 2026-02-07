package tunnel

// QUICOptions are server-side QUIC settings.
//
// If CertFile/KeyFile are empty, prisms may generate a self-signed certificate
// at startup.
//
// This is intentionally minimal; advanced policies (mTLS, CA pinning, etc.)
// can be added later.
type QUICOptions struct {
	CertFile string
	KeyFile  string

	// NextProtos is used for ALPN.
	// If empty, a Prism default is used.
	NextProtos []string
}

// QUICDialOptions are client-side QUIC settings.
type QUICDialOptions struct {
	ServerName string

	// InsecureSkipVerify allows connecting to a server with a self-signed
	// certificate. This is convenient for LAN / homelab deployments but should
	// be disabled in production.
	InsecureSkipVerify bool

	// NextProtos is used for ALPN.
	// If empty, a Prism default is used.
	NextProtos []string
}
