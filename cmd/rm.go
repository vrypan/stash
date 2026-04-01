package cmd

import (
	"bufio"
	"fmt"
	"io"
	"os"
	"strings"

	"github.com/spf13/cobra"
	"stash/store"
)

func newRmCmd() *cobra.Command {
	var before string
	var force bool

	cmd := &cobra.Command{
		Use:           "rm <id>",
		Short:         "Remove entries",
		SilenceUsage:  true,
		SilenceErrors: true,
		Args: func(_ *cobra.Command, args []string) error {
			if before != "" {
				if len(args) != 0 {
					return fmt.Errorf("rm accepts either <id> or --before, not both")
				}
				return nil
			}
			if len(args) != 1 {
				return fmt.Errorf("rm requires exactly one <id> unless --before is used")
			}
			return nil
		},
		RunE: func(_ *cobra.Command, args []string) error {
			if before != "" {
				return runRmBefore(before, force)
			}
			return store.WithLock(func() error {
				id, err := store.Resolve(args[0])
				if err != nil {
					return err
				}
				return store.Remove(id)
			})
		},
	}
	cmd.Flags().StringVar(&before, "before", "", "Remove entries older than the referenced entry")
	cmd.Flags().BoolVarP(&force, "force", "f", false, "Do not prompt for confirmation")
	return cmd
}

func runRmBefore(ref string, force bool) error {
	return store.WithLock(func() error {
		id, err := store.Resolve(ref)
		if err != nil {
			return err
		}
		entries, err := store.OlderThan(id)
		if err != nil {
			return err
		}
		if len(entries) == 0 {
			return nil
		}
		if !force {
			ok, err := confirmRmBefore(ref, entries)
			if err != nil {
				return err
			}
			if !ok {
				return nil
			}
		}
		for _, m := range entries {
			if err := store.Remove(m.ID); err != nil {
				return err
			}
		}
		return nil
	})
}

func confirmRmBefore(ref string, entries []store.Meta) (bool, error) {
	fmt.Fprintf(os.Stderr, "Remove %d entr", len(entries))
	if len(entries) == 1 {
		fmt.Fprintf(os.Stderr, "y older than %s? [y/N] ", ref)
	} else {
		fmt.Fprintf(os.Stderr, "ies older than %s? [y/N] ", ref)
	}
	reader := bufio.NewReader(os.Stdin)
	reply, err := reader.ReadString('\n')
	if err != nil && err != io.EOF {
		return false, err
	}
	reply = strings.TrimSpace(strings.ToLower(reply))
	return reply == "y" || reply == "yes", nil
}
