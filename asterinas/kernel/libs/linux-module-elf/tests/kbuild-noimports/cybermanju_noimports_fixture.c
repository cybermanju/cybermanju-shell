// SPDX-License-Identifier: MPL-2.0
/* Build/inspect fixture only. It is not loaded or executed. */
#include <linux/init.h>
#include <linux/module.h>

static int __init cybermanju_noimports_init(void)
{
    return 0;
}

static void __exit cybermanju_noimports_exit(void)
{
}

module_init(cybermanju_noimports_init);
module_exit(cybermanju_noimports_exit);
MODULE_LICENSE("GPL");
MODULE_VERSION("0.1.0");
MODULE_INFO(cybermanju_test_only, "ELF no-import fixture; not loader/security validation");
