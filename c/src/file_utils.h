#ifndef FILE_UTILS_H
#define FILE_UTILS_H

#include <stdbool.h>

typedef struct {
    char *name;
    char *path;
    char *hash;
} FileEntry;

typedef struct {
    FileEntry *entries;
    int count;
} FileList;

FileList read_directory(const char *folder, const char *algo);
void free_file_list(FileList list);
void compare_folders(const char *folder1, const char *folder2, const char *algo);

#endif
