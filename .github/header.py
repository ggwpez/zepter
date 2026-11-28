# Check that all rust files in the paths have a license header

import os
import sys

print("CWD: " + os.getcwd())

def rust_files(paths):
	for path in paths:
		for root, _, files in os.walk(path):
			for name in files:
				if name.endswith(".rs"):
					yield os.path.join(root, name)

def main():
	# args are the paths to check
	header = sys.argv[1]
	paths = sys.argv[2:]
	missing = []

	with open(header, "r") as f:
		header = f.read()

	for path in rust_files(paths):
		print("Checking " + path)
		with open(path, "r") as f:
			if not f.read().startswith(header):
				missing.append(path)

	if missing:
		print("The following files are missing a license header:")
		for file in missing:
			print(file)
		sys.exit(1)

if __name__ == "__main__":
	main()
