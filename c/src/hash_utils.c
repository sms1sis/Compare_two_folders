#include "hash_utils.h"
#include <openssl/sha.h>
#include <blake3.h>
#include <stdio.h>
#include <stdlib.h>

void compute_sha256(const char *path, char *output) {
    FILE *file = fopen(path, "rb");
    if (!file) {
        snprintf(output, 65, "ERROR");
        return;
    }

    SHA256_CTX ctx;
    SHA256_Init(&ctx);

    unsigned char buffer[4096];
    size_t len;
    while ((len = fread(buffer, 1, sizeof(buffer), file)) > 0) {
        SHA256_Update(&ctx, buffer, len);
    }

    unsigned char hash[SHA256_DIGEST_LENGTH];
    SHA256_Final(hash, &ctx);
    fclose(file);

    for (int i = 0; i < SHA256_DIGEST_LENGTH; i++) {
        sprintf(output + (i * 2), "%02x", hash[i]);
    }
    output[64] = '\0';
}

void compute_blake3(const char *path, char *output) {
    FILE *file = fopen(path, "rb");
    if (!file) {
        snprintf(output, 65, "ERROR");
        return;
    }

    blake3_hasher hasher;
    blake3_hasher_init(&hasher);

    unsigned char buffer[4096];
    size_t len;
    while ((len = fread(buffer, 1, sizeof(buffer), file)) > 0) {
        blake3_hasher_update(&hasher, buffer, len);
    }

    unsigned char hash[BLAKE3_OUT_LEN];
    blake3_hasher_finalize(&hasher, hash, BLAKE3_OUT_LEN);
    fclose(file);

    for (int i = 0; i < BLAKE3_OUT_LEN; i++) {
        sprintf(output + (i * 2), "%02x", hash[i]);
    }
    output[64] = '\0';
}
