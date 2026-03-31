package cmd

import (
	"github.com/spf13/cobra"
	"stash/store"
)

func newClearCmd() *cobra.Command {
	return &cobra.Command{
		Use:           "clear",
		Short:         "Remove all entries",
		SilenceUsage:  true,
		SilenceErrors: true,
		RunE: func(_ *cobra.Command, _ []string) error {
			return store.WithLock(store.Clear)
		},
	}
}
