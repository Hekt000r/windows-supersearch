``/engine/scanner.rs``
(OLD NOTE)
```rust
            /* We have to calculate the distance between the current position
            (usually boot sector) and the target position (MFT start)
            We do this by taking the byte-location of the MFT
            start and subtracting it by the MFT start (PHYSICAL BYTE OFFSET)
            That gives us the number of bytes we have to move (forward/positive)
            */
```