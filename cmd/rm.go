package cmd

import (
	"github.com/spf13/cobra"
	"stash/store"
)

func newRmCmd() *cobra.Command {
	return &cobra.Command{
		Use:           "rm <id>",
		Short:         "Remove a specific entry",
		Args:          cobra.ExactArgs(1),
		SilenceUsage:  true,
		SilenceErrors: true,
		RunE: func(_ *cobra.Command, args []string) error {
			return store.WithLock(func() error {
				id, err := store.Resolve(args[0])
				if err != nil {
					return err
				}
				return store.Remove(id)
			})
		},
	}
}
