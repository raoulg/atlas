"""A job runner: runs a task, says when a job is done, settles its log and stops the run."""

from jobs.runner import main, settle

__all__ = ["main", "settle"]
