package cmd

import (
	"fmt"
	"os"
	"path/filepath"
	"strings"

	"github.com/spf13/cobra"
	"stash/store"
)

func newPushCmd() *cobra.Command {
	var metaFlags []string

	cmd := &cobra.Command{
		Use:           "push [file]",
		Short:         "Read stdin and create a new entry (default command)",
		SilenceUsage:  true,
		SilenceErrors: true,
		Args:          cobra.MaximumNArgs(1),
		RunE: func(c *cobra.Command, args []string) error {
			return runPushWithMeta(c, args, metaFlags)
		},
	}

	cmd.Flags().StringArrayVarP(&metaFlags, "meta", "m", nil, "Metadata key=value (repeatable)")
	return cmd
}

func runPush(c *cobra.Command, args []string) error {
	return runPushWithMeta(c, args, nil)
}

func runPushWithMeta(_ *cobra.Command, args []string, metaFlags []string) error {
	var attrs map[string]string
	if len(metaFlags) > 0 {
		attrs = make(map[string]string, len(metaFlags))
		for _, kv := range metaFlags {
			k, v, ok := strings.Cut(kv, "=")
			if !ok {
				return fmt.Errorf("invalid --meta value %q: expected key=value", kv)
			}
			attrs[k] = v
		}
	}

	var (
		r   *os.File
		err error
	)
	if len(args) == 1 {
		r, err = os.Open(args[0])
		if err != nil {
			return err
		}
		defer r.Close()
		if attrs == nil {
			attrs = make(map[string]string, 1)
		}
		if _, ok := attrs["filename"]; !ok {
			attrs["filename"] = filepath.Base(args[0])
		}
	} else {
		stat, err := os.Stdin.Stat()
		if err != nil {
			return err
		}
		if stat.Mode()&os.ModeCharDevice != 0 {
			return fmt.Errorf("no stdin provided")
		}
		r = os.Stdin
	}

	id, err := store.Push(r, attrs)
	if err != nil {
		return err
	}
	fmt.Println(strings.ToLower(id))
	return nil
}
