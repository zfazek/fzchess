import chess
import re
import subprocess
import time

# ==================== CONFIGURATION ====================
ENGINE_PATH = "stockfish"  # Change to your engine's filename (e.g., engine.exe)
ENGINE_PATH = "./rust/target/release/fzchess"  # Change to your engine's filename (e.g., engine.exe)
EPD_FILE_PATH = "bratko-kopec.epd"        # Your EPD file path
EPD_FILE_PATH = "wac.epd"        # Your EPD file path
MOVETIME_MS = 5000                        # Time given per position in milliseconds (1 second)
# =======================================================

def send_uci_command(process, command, wait_for=None):
    """Sends a command to the engine and optionally waits for a specific response."""
    # Debug print to see what Python sends
    # print(f"-> Sending: {command}") 
    process.stdin.write(f"{command}\n")
    process.stdin.flush()
    
    if not wait_for:
        return []

    lines = []
    while True:
        line = process.stdout.readline()
        if not line: # Engine process died or closed stream
            break
        line = line.strip()
        
        # Un-comment this line to see EVERY line your Rust engine prints!
        # print(f"<- Engine: {line}") 
        
        lines.append(line)
        if line.startswith(wait_for):
            break
    return lines

def parse_epd_line(line):
    """Extracts the FEN, best moves (bm), and position ID from an EPD line."""
    # Split the EPD string by semicolons to separate commands
    parts = line.strip().split(';')
    if not parts:
        return None, None, None

    # The first part contains the FEN and usually the first opcode (like bm)
    main_part = parts[0]
    
    # Extract Best Move (bm) using regex
    bm_match = re.search(r'\bbm\s+([^;]+)', line)
    best_moves = [m.strip() for m in bm_match.group(1).split(',')] if bm_match else []

    # Extract ID if present
    id_match = re.search(r'\bid\s+"([^"]+)"', line)
    pos_id = id_match.group(1) if id_match else "Unknown"

    # Reconstruct clean FEN (strip out the opcodes from the end of the board string)
    # EPD shares the first 4 fields of FEN: <pieces> <color> <castling> <en_passant>
    fen_parts = main_part.split()
    # If the opcode was glued to the 4th field, isolate the first 4 elements
    clean_fen = " ".join(fen_parts[:4])

    return clean_fen, best_moves, pos_id

def run_test_suite():
    print(f"--> Starting test suite using: {ENGINE_PATH}")
    
    # Start the engine process
    try:
        engine = subprocess.Popen(
            ENGINE_PATH,
            stdin=subprocess.PIPE,
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
            text=True,
            bufsize=1
        )
    except FileNotFoundError:
        print(f"Error: Could not find the engine executable at '{ENGINE_PATH}'")
        return

    # Initialize UCI state
    send_uci_command(engine, "uci", wait_for="uciok")
    send_uci_command(engine, "isready", wait_for="readyok")

    score = 0.0
    total_positions = 0

    with open(EPD_FILE_PATH, 'r') as f:
        lines = f.readlines()

    for line in lines:
        line = line.strip()
        if not line or line.startswith('#'):
            continue

        fen, expected_moves, pos_id = parse_epd_line(line)
        if not fen or not expected_moves:
            continue

        total_positions += 1
        print(f"\n[{pos_id}] Testing Position {total_positions}...")

        # Clear engine hash/state for neutrality
        send_uci_command(engine, "ucinewgame")
        send_uci_command(engine, "isready", wait_for="readyok")

        # Set the board position
        send_uci_command(engine, f"position fen {fen}")

        # Tell engine to think -> CRUCIAL: Must wait explicitly for "bestmove"
        engine_output = send_uci_command(engine, f"go movetime {MOVETIME_MS}", wait_for="bestmove")
        
        # Parse the chosen move from the returned lines
        chosen_move = None
        for output_line in engine_output:
            if output_line.startswith("bestmove"):
                parts = output_line.split()
                if len(parts) > 1:
                    chosen_move = parts[1] # Extracts 'd8d1' from 'bestmove d8d1 ...'
                break

        print(f"  Expected (SAN): {expected_moves}")
        print(f"  Engine played:  {chosen_move}")

        # Set up the board with the position's FEN
        board = chess.Board(fen)

        try:
            # Convert the expected EPD SAN moves (e.g., 'Qd1+') into UCI strings (e.g., 'd8d1')
            expected_coordinate_moves = []
            for san_move in expected_moves:
                parsed_move = board.parse_san(san_move)
                expected_coordinate_moves.append(parsed_move.uci())
                
            # Compare the engine's string output directly to the converted list
            if chosen_move in expected_coordinate_moves:
                print("  Result: MATCH (+1.0)")
                score += 1.0
            else:
                print("  Result: MISSED (0.0)")
                
        except ValueError:

            # Fallback: if the file was already in LAN format, compare strings directly
            if chosen_move in expected_moves:
                print("  Result: MATCH (+1.0)")
                score += 1.0
            else:
                print("  Result: MISSED (0.0)")

    # Clean up and close engine
    send_uci_command(engine, "quit")
    engine.terminate()

    # Final tally
    if total_positions > 0:
        accuracy = (score / total_positions) * 100
        print("\n================ TEST SUMMARY ================")
        print(f"Total Positions: {total_positions}")
        print(f"Correct Answers: {score}")
        print(f"Overall Accuracy: {accuracy:.2f}%")
        print("==============================================")
    else:
        print("No valid positions parsed.")

if __name__ == "__main__":
    run_test_suite()

