package cmd

import (
	"errors"
	"fmt"
	"os"

	"github.com/spf13/cobra"
	"stash/store"
)

var rootAttrFlags []string

var rootCmd = &cobra.Command{
	Use:           "stash [file]",
	Short:         "A local store for pipeline output",
	Args:          cobra.MaximumNArgs(1),
	SilenceUsage:  true,
	SilenceErrors: true,
	RunE: func(c *cobra.Command, args []string) error {
		return runPushWithAttrs(c, args, rootAttrFlags)
	},
}

func init() {
	rootCmd.Flags().StringArrayVarP(&rootAttrFlags, "attr", "a", nil, "Attribute key=value (repeatable)")
	rootCmd.AddCommand(
		newPushCmd(),
		newTeeCmd(),
		newLogCmd(),
		newLsCmd(),
		newAttrCmd(),
		newPopCmd(),
		newCatCmd(),
		newPathCmd(),
		newRmCmd(),
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
	var partial *store.ErrPartialSaved
	switch {
	case errors.Is(err, store.ErrEmpty):
		return 1
	case errors.As(err, &nf):
		return 2
	case errors.As(err, &amb):
		return 3
	case errors.As(err, &partial):
		return 4
	default:
		return 1
	}
}
