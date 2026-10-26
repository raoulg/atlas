"""Rows kept in files, and pages made from a template the package ships."""

from stock.pages import page, resource_page
from stock.rows import default_rows, read_rows, save, save_or_default, save_or_given, save_where_told

__all__ = ["default_rows", "page", "read_rows", "resource_page", "save", "save_or_default", "save_or_given", "save_where_told"]
