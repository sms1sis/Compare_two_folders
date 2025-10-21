CC = gcc
CFLAGS = -Wall -Iinclude -Iinclude/cJSON -I$PREFIX/include -Ivendor/blake3/c
LDFLAGS = -Lvendor/blake3/c/build -lblake3 -lcrypto

SRC = src/main.c \
      src/file_utils.c \
      src/hash_utils.c \
      src/report.c \
      src/json_utils.c \
      src/color.c \
      include/cJSON/cJSON.c

OBJ = $(SRC:.c=.o)
BIN = compare_folders

all: $(BIN)

$(BIN): $(OBJ)
	$(CC) $(OBJ) -o $(BIN) $(LDFLAGS)

clean:
	rm -f $(OBJ) $(BIN)
