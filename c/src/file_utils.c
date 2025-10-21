#include "file_utils.h"
#include "hash_utils.h"
#include "color.h"
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <dirent.h>
#include <sys/stat.h>

FileList read_directory(const char *folder, const char *algo) {
    FileList list = { .entries = NULL, .count = 0 };
    DIR *dir = opendir(folder);
    if (!dir) {
        fprintf(stderr, "Failed to open directory: %s\n", folder);
        return list;
    }

    struct dirent *entry;
    while ((entry = readdir(dir)) != NULL) {
        if (entry->d_type != DT_REG) continue;

        char full_path[1024];
        snprintf(full_path, sizeof(full_path), "%s/%s", folder, entry->d_name);

        char *hash = malloc(65);
        if (strcmp(algo, "sha256") == 0) {
            compute_sha256(full_path, hash);
        } else if (strcmp(algo, "blake3") == 0) {
            compute_blake3(full_path, hash);
        } else {
            char hash1[65], hash2[65];
            compute_sha256(full_path, hash1);
            compute_blake3(full_path, hash2);
            snprintf(hash, 129, "%s|%s", hash1, hash2);
        }

        list.entries = realloc(list.entries, sizeof(FileEntry) * (list.count + 1));
        list.entries[list.count].name = strdup(entry->d_name);
        list.entries[list.count].path = strdup(full_path);
        list.entries[list.count].hash = hash;
        list.count++;
    }

    closedir(dir);
    return list;
}

void free_file_list(FileList list) {
    for (int i = 0; i < list.count; i++) {
        free(list.entries[i].name);
        free(list.entries[i].path);
        free(list.entries[i].hash);
    }
    free(list.entries);
}

void compare_folders(const char *folder1, const char *folder2, const char *algo) {
    FileList list1 = read_directory(folder1, algo);
    FileList list2 = read_directory(folder2, algo);

    int matched = 0, unmatched = 0;

    for (int i = 0; i < list1.count; i++) {
        bool found = false;
        for (int j = 0; j < list2.count; j++) {
            if (strcmp(list1.entries[i].name, list2.entries[j].name) == 0 &&
                strcmp(list1.entries[i].hash, list2.entries[j].hash) == 0) {
                found = true;
                break;
            }
        }

        if (found) {
            printf(COLOR_GREEN "MATCHED: %s\n" COLOR_RESET, list1.entries[i].name);
            matched++;
        } else {
            printf(COLOR_RED "UNMATCHED: %s\n" COLOR_RESET, list1.entries[i].name);
            unmatched++;
        }
    }

    printf("\nSummary: %d matched, %d unmatched\n", matched, unmatched);
    free_file_list(list1);
    free_file_list(list2);
}
