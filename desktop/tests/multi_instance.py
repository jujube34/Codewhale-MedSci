"""Compatibility entry point for the native multi-instance session checks.

The former desktop-store scanner was removed. Coverage lives in the native
session fixture so this entry point cannot keep exercising the retired model.
"""
from session_interop import main

if __name__ == "__main__":
    main()
