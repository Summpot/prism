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
//
// JSON config files are intentionally not supported because JSON has no
// comments and Prism configs are expected to be annotated.
func DiscoverConfigPath(dir string) (string, error) {
	candidates := CandidateConfigPaths(dir)
	for _, p := range candidates {
		if isRegularFile(p) {
			return p, nil
		}
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
	}
}

func isRegularFile(path string) bool {
	fi, err := os.Stat(path)
	if err != nil {
		return false
	}
	return fi.Mode().IsRegular()
}
