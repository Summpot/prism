package main

import (
	"context"
	"flag"
	"fmt"
	"os"
	"os/signal"
	"syscall"

	"prism/internal/app"
)

func main() {
	var (
		configPath = flag.String("config", "", "Path to Prism config file (.toml/.yaml/.yml). If empty, uses PRISM_CONFIG; then auto-detects prism.toml > prism.yaml > prism.yml from CWD; then falls back to the OS default user config path")
	)
	flag.Parse()

	ctx, stop := signal.NotifyContext(context.Background(), os.Interrupt, syscall.SIGTERM)
	defer stop()

	if err := app.RunPrism(ctx, *configPath); err != nil {
		_, _ = fmt.Fprintf(os.Stderr, "%v\n", err)
		os.Exit(1)
	}
}
