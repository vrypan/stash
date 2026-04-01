package cmd

import (
	"fmt"

	"github.com/spf13/cobra"
	"stash/store"
)

func newIndexCmd() *cobra.Command {
	cmd := &cobra.Command{
		Use:           "index",
		Short:         "Manage stash indexes",
		SilenceUsage:  true,
		SilenceErrors: true,
	}

	cmd.AddCommand(&cobra.Command{
		Use:           "update",
		Short:         "Rebuild entry indexes from the stash directory",
		Args:          cobra.NoArgs,
		SilenceUsage:  true,
		SilenceErrors: true,
		RunE: func(_ *cobra.Command, _ []string) error {
			return store.WithLock(func() error {
				n, err := store.UpdateIndex()
				if err != nil {
					return err
				}
				fmt.Printf("indexed %d entries\n", n)
				return nil
			})
		},
	})

	return cmd
}
