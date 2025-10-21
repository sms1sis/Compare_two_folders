#include <stdio.h>
#include "file_utils.h"
#include "hash_utils.h"
#include "report.h"
#include "color.h"
#include "json_utils.h"
#include <string.h>

int main(int argc, char *argv[]) {
    if (argc < 3) {
        printf("Usage: %s <folder1> <folder2> [--algo=blake3|sha256|both] [--json]\n", argv[0]);
        return 1;
    }

    const char *folder1 = argv[1];
    const char *folder2 = argv[2];
    const char *algo = "both";
    bool json_output = false;

    for (int i = 3; i < argc; i++) {
        if (strstr(argv[i], "--algo=")) {
            algo = argv[i] + 7;
        } else if (strcmp(argv[i], "--json") == 0) {
            json_output = true;
        }
    }

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

    print_summary(matched, unmatched);

    if (json_output) {
        write_json_report("report.json", list1, list2);
        printf("📝 JSON report written to report.json\n");
    }

    free_file_list(list1);
    free_file_list(list2);
    return 0;
}