#include "report.h"
#include <stdio.h>

void print_summary(int matched, int unmatched) {
    printf("\n==================== Summary ====================\n");
    printf("Matched files   : %d\n", matched);
    printf("Unmatched files : %d\n", unmatched);
    printf("=================================================\n");
}
