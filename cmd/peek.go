package cmd

import (
	"os"

	"github.com/spf13/cobra"
	"stash/store"
)

func newPeekCmd() *cobra.Command {
	return &cobra.Command{
		Use:           "peek",
		Short:         "Write the most recent entry's content to stdout without removing it",
		SilenceUsage:  true,
		SilenceErrors: true,
		RunE: func(_ *cobra.Command, _ []string) error {
			m, err := store.Newest()
			if err != nil {
				return err
			}
			return store.Cat(m.ID, os.Stdout)
		},
	}
}
