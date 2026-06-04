# Installing TA-Lib for Golden Vector Generation

To generate golden vectors, install the TA-Lib C library and Python bindings.

## Linux (Ubuntu/Debian)

```bash
# Install TA-Lib C library
sudo apt-get update
sudo apt-get install build-essential
sudo apt-get install libta-lib0-dev

# Install Python dependencies
python3 -m pip install --user numpy TA-Lib
```

## macOS

```bash
# Install TA-Lib C library using Homebrew
brew install ta-lib

# Install Python dependencies
pip install numpy TA-Lib
```

## Windows

1. **Download TA-Lib:**
   - Visit https://ta-lib.org/download.html
   - Download the Windows binary (e.g., `ta-lib-0.4.0-msvc.zip`)
   - Extract to `C:\ta-lib`

2. **Set Environment Variable:**
   ```cmd
   set TA_LIBRARY_PATH=C:\ta-lib\c\lib
   set TA_INCLUDE_PATH=C:\ta-lib\c\include
   ```

3. **Install Python dependencies:**
   ```cmd
   pip install numpy TA-Lib
   ```

## Verification

After installation, verify TA-Lib is working:

```python
import talib
import numpy as np

# Test data
data = np.array([100.0, 101.0, 102.0, 103.0, 104.0], dtype=np.float64)

# Test RSI
rsi = talib.RSI(data, timeperiod=3)
print("RSI:", rsi)

# Test EMA
ema = talib.EMA(data, timeperiod=3)
print("EMA:", ema)
```

## Generating Vectors

```bash
python3 tflo-ta-golden/scripts/generate_talib_vectors.py --all --output-dir tflo-ta-golden/fixtures
```

This produces TA-Lib vectors for all indicators — the single source of truth
for validating `tflo-ta-strict` implementations.

## Troubleshooting

### "No module named 'talib'"
- Ensure TA-Lib C library is installed first
- Try: `pip install --upgrade TA-Lib`
- On Linux, ensure `libta-lib0-dev` is installed

### "Cannot find ta-lib.h"
- Ensure TA-Lib C library headers are in the expected location
- On Linux: `/usr/include/ta-lib/`
- On macOS: `/usr/local/include/ta-lib/`
- On Windows: Set `TA_INCLUDE_PATH` environment variable

### Import errors
- Try reinstalling: `pip uninstall TA-Lib && pip install TA-Lib`
- Check Python version compatibility (TA-Lib requires Python 3.6+)

## Alternative: Using Docker

If installation is problematic, use Docker:

```dockerfile
FROM python:3.11-slim

RUN apt-get update && \
    apt-get install -y build-essential libta-lib0-dev && \
    pip install numpy TA-Lib

WORKDIR /workspace
COPY . .
CMD ["python3", "tflo-ta-golden/scripts/generate_talib_vectors.py", "--all"]
```

```bash
docker build -t tflo-talib .
docker run -v $(pwd)/tflo-ta-golden/fixtures:/workspace/fixtures tflo-talib
```

