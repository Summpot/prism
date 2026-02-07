package config

import (
	"encoding/json"
	"os"
	"path/filepath"
	"testing"
)

func TestPrismSchemaJSONIsValid(t *testing.T) {
	// Tests run with the package directory as the working directory.
	p := filepath.Join("..", "..", "prism.schema.json")
	b, err := os.ReadFile(p)
	if err != nil {
		t.Fatalf("read schema %q: %v", p, err)
	}
	if !json.Valid(b) {
		t.Fatalf("schema %q is not valid JSON", p)
	}

	var m map[string]any
	if err := json.Unmarshal(b, &m); err != nil {
		t.Fatalf("unmarshal schema %q: %v", p, err)
	}
	if m["$schema"] == nil {
		t.Fatalf("schema %q missing $schema", p)
	}
	if m["properties"] == nil {
		t.Fatalf("schema %q missing properties", p)
	}
}
