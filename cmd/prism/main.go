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
		configPath = flag.String("config", "", "Path to Prism config file (.toml/.yaml/.yml). If empty, auto-detect prism.toml > prism.yaml > prism.yml")
	)
	flag.Parse()

	ctx, stop := signal.NotifyContext(context.Background(), os.Interrupt, syscall.SIGTERM)
	defer stop()

	if err := app.RunPrisms(ctx, *configPath); err != nil {
		_, _ = fmt.Fprintf(os.Stderr, "%v\n", err)
		os.Exit(1)
	}
}
