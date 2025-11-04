import json
import sys

def extract_matrix(data):
    """
    Extracts numeric matrix values from the nested AbstractLiteral/Matrix JSON structure.
    """
    # data = the root JSON list
    result = []

    # Go through each solution
    for sol in data:
        # Each solution has variables like "a", "b", etc.
        for var_name, var_data in sol.items():
            # Navigate into: AbstractLiteral -> Matrix -> [0]
            matrix = var_data["AbstractLiteral"]["Matrix"][0]
            flat_matrix = []

            for cell in matrix:
                # Each cell is an AbstractLiteral containing a nested Matrix
                # Navigate to Int value: ["AbstractLiteral"]["Matrix"][0][0]["Int"]
                try:
                    val = cell["AbstractLiteral"]["Matrix"][0][0]["Int"]
                except (KeyError, IndexError, TypeError):
                    val = None
                flat_matrix.append(val)

            result.append({var_name: flat_matrix})
    return result


def main():
    if len(sys.argv) < 3:
        print("Usage: python convert_to_matrix.py input.json output.json")
        sys.exit(1)

    input_path = sys.argv[1]
    output_path = sys.argv[2]

    # Read input JSON
    with open(input_path, "r") as f:
        data = json.load(f)

    # Convert structure
    matrix_data = extract_matrix(data)

    # Write to output file (pretty formatted)
    with open(output_path, "w") as f:
        json.dump(matrix_data, f, indent=2)

    print(f"✅ Converted matrix written to {output_path}")


if __name__ == "__main__":
    main()
