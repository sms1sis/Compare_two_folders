# Makefile for the Folder File Comparison Utility (C Version)

# Compiler and flags
CC = gcc
CFLAGS = -Wall -Wextra -O2

# Libraries
LDLIBS = -lssl -lcrypto

# Source and target files
SRC = comtwofolsha.c
TARGET = compare_folders_c

# Default target
all: $(TARGET)

$(TARGET): $(SRC)
	$(CC) $(CFLAGS) -o $(TARGET) $(SRC) $(LDLIBS)

# Clean up build artifacts
clean:
	rm -f $(TARGET)

.PHONY: all clean
