#ifndef HASH_UTILS_H
#define HASH_UTILS_H

void compute_sha256(const char *path, char *output);
void compute_blake3(const char *path, char *output);

#endif
