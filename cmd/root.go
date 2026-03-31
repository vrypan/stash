package cmd

import (
	"errors"
	"fmt"
	"os"

	"github.com/spf13/cobra"
	"stash/store"
)

var rootCmd = &cobra.Command{
	Use:           "stash",
	Short:         "A local store for pipeline output",
	SilenceUsage:  true,
	SilenceErrors: true,
	RunE:          runPush,
}

func init() {
	rootCmd.AddCommand(
		newPushCmd(),
		newListCmd(),
		newPeekCmd(),
		newPopCmd(),
		newCatCmd(),
		newRmCmd(),
		newClearCmd(),
		newVersionCmd(),
	)
}

// Execute runs the root command and exits with the appropriate code on error.
func Execute() {
	if err := rootCmd.Execute(); err != nil {
		fmt.Fprintln(os.Stderr, "error:", err)
		os.Exit(exitCode(err))
	}
}

func exitCode(err error) int {
	var nf *store.ErrNotFound
	var amb *store.ErrAmbiguous
	switch {
	case errors.Is(err, store.ErrEmpty):
		return 1
	case errors.As(err, &nf):
		return 2
	case errors.As(err, &amb):
		return 3
	default:
		return 1
	}
}
