#include "json_utils.h"
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include "cJSON.h"

void write_json_report(const char *filename, FileList list1, FileList list2) {
    cJSON *root = cJSON_CreateObject();
    cJSON *matched = cJSON_CreateArray();
    cJSON *unmatched = cJSON_CreateArray();

    for (int i = 0; i < list1.count; i++) {
        bool found = false;
        for (int j = 0; j < list2.count; j++) {
            if (strcmp(list1.entries[i].name, list2.entries[j].name) == 0 &&
                strcmp(list1.entries[i].hash, list2.entries[j].hash) == 0) {
                found = true;
                break;
            }
        }

        cJSON *entry = cJSON_CreateObject();
        cJSON_AddStringToObject(entry, "name", list1.entries[i].name);
        cJSON_AddStringToObject(entry, "hash", list1.entries[i].hash);

        if (found) {
            cJSON_AddItemToArray(matched, entry);
        } else {
            cJSON_AddItemToArray(unmatched, entry);
        }
    }

    cJSON_AddItemToObject(root, "matched", matched);
    cJSON_AddItemToObject(root, "unmatched", unmatched);

    char *json_str = cJSON_Print(root);
    FILE *file = fopen(filename, "w");
    if (file) {
        fprintf(file, "%s\n", json_str);
        fclose(file);
    }

    free(json_str);
    cJSON_Delete(root);
}
