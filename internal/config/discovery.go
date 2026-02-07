package config

import (
	"fmt"
	"os"
	"path/filepath"
)

// DiscoverConfigPath finds the configuration file in dir using Prism's default
// naming convention and precedence.
//
// Precedence:
//  1. prism.toml
//  2. prism.yaml
//  3. prism.yml
//  4. prism.json
//
// For backward compatibility, if none of the prism.* files exist, it will fall
// back to config.json.
func DiscoverConfigPath(dir string) (string, error) {
	candidates := CandidateConfigPaths(dir)
	for _, p := range candidates {
		if isRegularFile(p) {
			return p, nil
		}
	}

	legacy := filepath.Join(dir, "config.json")
	if isRegularFile(legacy) {
		return legacy, nil
	}

	return "", fmt.Errorf("no config file found in %s; looked for %v", dir, candidates)
}

func CandidateConfigPaths(dir string) []string {
	return CandidateConfigPathsForBase(dir, "prism")
}

func DiscoverConfigPathForBase(dir, base string) (string, error) {
	candidates := CandidateConfigPathsForBase(dir, base)
	for _, p := range candidates {
		if isRegularFile(p) {
			return p, nil
		}
	}

	// Legacy fallback only applies to the server.
	if base == "prism" {
		legacy := filepath.Join(dir, "config.json")
		if isRegularFile(legacy) {
			return legacy, nil
		}
	}

	return "", fmt.Errorf("no config file found in %s; looked for %v", dir, candidates)
}

func CandidateConfigPathsForBase(dir, base string) []string {
	base = filepath.Base(base)
	if base == "" {
		base = "prism"
	}
	return []string{
		filepath.Join(dir, base+".toml"),
		filepath.Join(dir, base+".yaml"),
		filepath.Join(dir, base+".yml"),
		filepath.Join(dir, base+".json"),
	}
}

func isRegularFile(path string) bool {
	fi, err := os.Stat(path)
	if err != nil {
		return false
	}
	return fi.Mode().IsRegular()
}
