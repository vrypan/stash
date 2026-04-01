package cmd

import (
	"errors"
	"fmt"
	"os"
	"strings"

	"github.com/spf13/cobra"
	"stash/store"
)

func newTeeCmd() *cobra.Command {
	var metaFlags []string
	var partial bool

	cmd := &cobra.Command{
		Use:           "tee",
		Short:         "Stream stdin to stdout and stash it at the same time",
		Args:          cobra.NoArgs,
		SilenceUsage:  true,
		SilenceErrors: true,
		RunE: func(_ *cobra.Command, _ []string) error {
			stat, err := os.Stdin.Stat()
			if err != nil {
				return err
			}
			if stat.Mode()&os.ModeCharDevice != 0 {
				return fmt.Errorf("no stdin provided")
			}

			attrs, err := parseMetaFlags(metaFlags)
			if err != nil {
				return err
			}

			id, err := store.Tee(os.Stdin, os.Stdout, attrs, partial)
			if err == nil {
				fmt.Fprintln(os.Stderr, strings.ToLower(id))
				return nil
			}

			var partialErr *store.ErrPartialSaved
			if errors.As(err, &partialErr) {
				fmt.Fprintf(os.Stderr, "partial stash saved: %s\n", strings.ToLower(partialErr.ID))
				return partialErr
			}
			return err
		},
	}

	cmd.Flags().StringArrayVarP(&metaFlags, "meta", "m", nil, "Metadata key=value (repeatable)")
	cmd.Flags().BoolVar(&partial, "partial", false, "Save a partial entry if the stream is interrupted")
	return cmd
}
