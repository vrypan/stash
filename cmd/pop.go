package cmd

import (
	"os"

	"github.com/spf13/cobra"
	"stash/store"
)

func newPopCmd() *cobra.Command {
	return &cobra.Command{
		Use:           "pop",
		Short:         "Write the most recent entry to stdout and remove it",
		SilenceUsage:  true,
		SilenceErrors: true,
		RunE: func(_ *cobra.Command, _ []string) error {
			return store.WithLock(func() error {
				m, err := store.Newest()
				if err != nil {
					return err
				}
				if err := store.Cat(m.ID, os.Stdout); err != nil {
					return err
				}
				return store.Remove(m.ID)
			})
		},
	}
}
