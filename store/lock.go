package store

import (
	"fmt"
	"os"
	"syscall"
)

// WithLock acquires an exclusive flock on the repo lock file,
// runs fn, then releases the lock.
func WithLock(fn func() error) error {
	lp, err := LockFilePath()
	if err != nil {
		return err
	}
	f, err := os.OpenFile(lp, os.O_CREATE|os.O_RDWR, 0600)
	if err != nil {
		return fmt.Errorf("open lock: %w", err)
	}
	defer f.Close()
	if err := syscall.Flock(int(f.Fd()), syscall.LOCK_EX); err != nil {
		return fmt.Errorf("acquire lock: %w", err)
	}
	defer syscall.Flock(int(f.Fd()), syscall.LOCK_UN) //nolint:errcheck
	return fn()
}
