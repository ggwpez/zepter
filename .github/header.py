# Check that all rust files in the paths have a license header

import os
import sys

print("CWD: " + os.getcwd())

def main():
	# args are the paths to check
	header = sys.argv[1]
	paths = sys.argv[2:]
	missing = []

	with open(header, "r") as f:
		header = f.read()

	for path in paths:
		for root, dirs, files in os.walk(path):
			for file in files:
				if file.endswith(".rs"):
					print("Checking " + os.path.join(root, file))
					with open(os.path.join(root, file), "r") as f:
						if not f.read().startswith(header):
							missing.append(os.path.join(root, file))
	
	if len(missing) > 0:
		print("The following files are missing a license header:")
		for file in missing:
			print(file)
		sys.exit(1)

if __name__ == "__main__":
	main()
